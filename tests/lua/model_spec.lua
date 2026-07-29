local model = require 'tmx_switcher.model'
local suite = {}

local function native()
  return {
    {kind='tab', domain='local', workspace='work', mux_window_id='1', tab_id='10', pane_id='100', title='API', path='/src/api', command='zsh', tty='/dev/ttys001', current=true},
    {kind='pane', domain='local', workspace='work', mux_window_id='1', tab_id='10', pane_id='100', title='shell', path='/src/api', command='zsh', tty='/dev/ttys001', current=true},
    {kind='pane', domain='local', workspace='other', mux_window_id='2', tab_id='20', pane_id='200', title='logs', path='/var/log', command='tail', tty='/dev/ttys002', current=false},
  }
end

local function inventory()
  return {
    schema={name='dev.tmx.inventory', major=1, minor=0}, request_id='r', producer_version='0.1',
    generated_at='now', applied_limits={}, complete=true,
    capabilities={'augmentation_enabled','clients','route_session','route_window','route_pane'}, diagnostics={},
    endpoints={{
      host_domain='local', endpoint_id='ep_a', alias='work', selector_kind='name', trust_source='configuration', status='available',
      generation={token='gen_a'}, diagnostics={},
      sessions={{endpoint_id='ep_a',generation='gen_a',session_id='$1',name='server',path='/src/api',created='1',activity='9',attached_count='1',window_count='1'}},
      windows={{endpoint_id='ep_a',generation='gen_a',session_id='$1',window_id='@1',index='0',name='editor',active=true,activity='8'}},
      panes={{endpoint_id='ep_a',generation='gen_a',session_id='$1',window_id='@1',pane_id='%1',index='0',active=true,activity='7',tty='/dev/ttys001',path='/src/api',command='nvim',title='code'}},
      clients={{endpoint_id='ep_a',generation='gen_a',client_name='/dev/ttys001',client_pid='10',client_created='11',client_tty='/dev/ttys001',client_uid='501',attached_session_id='$1',current_window_id='@1',current_pane_id='%1'}},
    }}
  }
end

local function labels(result)
  local out = {}
  for _, choice in ipairs(result.choices) do out[#out + 1] = choice.label end
  return table.concat(out, '\n')
end

function suite.sanitizer_is_idempotent_bounded_and_removes_controls()
  local hostile = '\27[31mred\n\tzero\0' .. string.rep('x', 1000)
  local once = model.sanitize(hostile, 64)
  local twice = model.sanitize(once, 64)
  assert(once == twice)
  assert(#once <= 64)
  assert(not once:find('\27', 1, true))
  assert(not once:find('\n', 1, true))
  assert(not once:find('\0', 1, true))
end

function suite.additive_minor_fields_are_accepted()
  local data = inventory()
  data.schema.minor = 9
  data.future_optional = {safe=true}
  data.endpoints[1].sessions[1].future_counter = 'wide'
  local result = model.build(native(), data, {current_workspace='work'})
  assert(#result.choices == 6)
  assert(#result.diagnostics == 0)
end

function suite.partial_inventory_keeps_healthy_tmux_and_all_native_rows()
  local data = inventory()
  data.complete = false
  data.endpoints[1].status = 'partial'
  data.endpoints[1].diagnostics = {{code='timeout',message='bounded',endpoint_id='ep_a'}}
  local result = model.build(native(), data, {current_workspace='work'})
  assert(#result.choices == 6)
  assert(#result.diagnostics == 1)
end

function suite.native_rows_survive_missing_or_invalid_tmux()
  local result = model.build(native(), {schema={name='bad',major=9}}, {current_workspace='work'})
  assert(#result.choices == 3)
  assert(#result.diagnostics == 1)
  assert(labels(result):match('%[wezterm tab%]'))
  assert(labels(result):match('%[wezterm pane%]'))
end

function suite.current_native_then_matched_tmux_then_other_native()
  local result = model.build(native(), inventory(), {current_workspace='work', allowed_local_domains={'local'}})
  assert(result.choices[1].label:match('%[current%]'))
  assert(result.choices[2].label:match('%[current%]'))
  assert(result.choices[3].label:match('%[tmux'))
  assert(result.choices[#result.choices].label:match('logs'))
  local route = result.routes[result.choices[3].id]
  assert(route.provider == 'tmux')
  assert(route.client.client_name == '/dev/ttys001')
  assert(route.host_pane.pane_id == '100')
end

function suite.remote_or_ambiguous_ttys_never_match_clients()
  local rows = native()
  rows[1].domain, rows[2].domain = 'ssh:host', 'ssh:host'
  local result = model.build(rows, inventory(), {current_workspace='work', allowed_local_domains={'local'}})
  for _, route in pairs(result.routes) do
    if route.provider == 'tmux' then assert(route.client == nil) end
  end
  rows = native()
  rows[#rows + 1] = {kind='pane',domain='local',workspace='work',mux_window_id='3',tab_id='30',pane_id='300',title='duplicate',tty='/dev/ttys001'}
  result = model.build(rows, inventory(), {current_workspace='work', allowed_local_domains={'local'}})
  for _, route in pairs(result.routes) do
    if route.provider == 'tmux' then assert(route.client == nil) end
  end
end

function suite.malformed_identity_hierarchy_and_object_shaped_arrays_are_rejected()
  local cases = {
    function(data) data.endpoints[1].windows[1].endpoint_id = 'ep_other' end,
    function(data) data.endpoints[1].panes[1].window_id = '@missing' end,
    function(data) data.endpoints[1].sessions[1].session_id = '1' end,
    function(data) data.endpoints[1].sessions = {only=data.endpoints[1].sessions[1]} end,
    function(data) data.endpoints[1].clients[1].current_pane_id = '%missing' end,
  }
  for _, mutate in ipairs(cases) do
    local data = inventory()
    mutate(data)
    local result = model.build(native(), data, {current_workspace='work'})
    assert(#result.choices == 3)
    assert(#result.diagnostics == 1)
  end
end

function suite.hostile_labels_are_sanitized_and_routes_remain_typed()
  local data = inventory()
  data.endpoints[1].panes[1].title = '\27[31mboom\n\t$(touch nope)'
  local result = model.build(native(), data, {current_workspace='work'})
  local output = labels(result)
  assert(not output:find('\27', 1, true))
  assert(not output:find('\n\t', 1, true))
  local pane_route
  for _, route in pairs(result.routes) do if route.provider == 'tmux' and route.kind == 'pane' then pane_route = route end end
  assert(pane_route.pane_id == '%1')
  assert(pane_route.pane_id ~= data.endpoints[1].panes[1].title)
end

function suite.colliding_runtime_ids_on_endpoints_stay_distinct()
  local data = inventory()
  local second = inventory().endpoints[1]
  second.endpoint_id, second.alias, second.generation.token = 'ep_b', 'personal', 'gen_b'
  for _, collection in ipairs({second.sessions,second.windows,second.panes,second.clients}) do
    for _, record in ipairs(collection) do record.endpoint_id, record.generation = 'ep_b', 'gen_b' end
  end
  table.insert(data.endpoints, second)
  local result = model.build(native(), data, {current_workspace='work'})
  local seen = {}
  for _, route in pairs(result.routes) do
    if route.provider == 'tmux' and route.kind == 'pane' then seen[route.endpoint_id] = true end
  end
  assert(seen.ep_a and seen.ep_b)
end

function suite.order_is_independent_of_input_permutation()
  local one = model.build(native(), inventory(), {current_workspace='work'})
  local reversed_native = native()
  for i=1, math.floor(#reversed_native/2) do reversed_native[i], reversed_native[#reversed_native-i+1] = reversed_native[#reversed_native-i+1], reversed_native[i] end
  local data = inventory()
  data.endpoints[1].panes[1], data.endpoints[1].panes[#data.endpoints[1].panes] = data.endpoints[1].panes[#data.endpoints[1].panes], data.endpoints[1].panes[1]
  local two = model.build(reversed_native, data, {current_workspace='work'})
  assert(labels(one) == labels(two))
end

function suite.utf8_truncation_and_long_duplicate_qualifiers_stay_bounded_and_visible()
  local shortened = model.sanitize(string.rep('界', 200), 512)
  assert(#shortened <= 512)
  assert(shortened:sub(-3) == '…')

  local rows = {
    {kind='pane',domain='local',workspace='same',mux_window_id='1',tab_id='1',pane_id='111',title=string.rep('x', 600)},
    {kind='pane',domain='local',workspace='same',mux_window_id='2',tab_id='2',pane_id='222',title=string.rep('x', 600)},
  }
  local result = model.build(rows, nil, {current_workspace='same'})
  assert(#result.choices == 2)
  assert(#result.choices[1].label <= 512 and #result.choices[2].label <= 512)
  assert(result.choices[1].label ~= result.choices[2].label)
  assert(result.choices[1].label:match('%[same 1 111%]') or result.choices[2].label:match('%[same 1 111%]'))
  assert(result.choices[1].label:match('%[same 2 222%]') or result.choices[2].label:match('%[same 2 222%]'))
end

function suite.equal_labels_remain_separate_and_qualified()
  local rows = native()
  rows[#rows + 1] = {kind='pane',domain='local',workspace='other',mux_window_id='9',tab_id='90',pane_id='900',title='logs',path='/var/log',command='tail',tty='/dev/ttys009'}
  local result = model.build(rows, nil, {current_workspace='work'})
  local count = 0
  for _, choice in ipairs(result.choices) do if choice.label:match('logs') then count = count + 1; assert(choice.label:match('%[')) end end
  assert(count == 2)
end

return suite
