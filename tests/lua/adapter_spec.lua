local adapter = require 'tmx_switcher'
local suite = {}

local function pane(id, tty, domain)
  return {
    pane_id=function() return id end,
    get_domain_name=function() return domain or 'local' end,
    get_title=function() return 'pane-' .. id end,
    get_current_working_dir=function() return '/tmp/' .. id end,
    get_foreground_process_name=function() return 'zsh' end,
    get_tty_name=function() return tty end,
    activate=function(self) self.activated = true end,
  }
end

local function mux_fixture(id, tty, workspace, window_id, tab_id)
  id, tty = id or 100, tty or '/dev/ttys001'
  workspace, window_id, tab_id = workspace or 'work', window_id or 1, tab_id or 10
  local p = pane(id, tty)
  local tab = {
    tab_id=function() return tab_id end,
    get_title=function() return 'tab' end,
    active_pane=function() return p end,
    panes=function() return {p} end,
  }
  local window = {
    window_id=function() return window_id end,
    get_workspace=function() return workspace end,
    tabs=function() return {tab} end,
  }
  return p, window
end

local function inventory_json(client_tty)
  return string.format([[{
    "schema":{"name":"dev.tmx.inventory","major":1,"minor":0},
    "request_id":"r","producer_version":"0.1","generated_at":"now",
    "applied_limits":{},"complete":true,
    "capabilities":["augmentation_enabled","clients","route_session","route_window","route_pane"],
    "diagnostics":[],"endpoints":[{
      "host_domain":"local","endpoint_id":"ep_a","alias":"work","selector_kind":"name","trust_source":"configuration",
      "status":"available","generation":{"token":"gen_a"},"diagnostics":[],
      "sessions":[{"endpoint_id":"ep_a","generation":"gen_a","session_id":"$1","name":"server","path":"/tmp","created":"1","activity":"9","attached_count":"1","window_count":"1"}],
      "windows":[{"endpoint_id":"ep_a","generation":"gen_a","session_id":"$1","window_id":"@1","index":"0","name":"editor","active":true,"activity":"8"}],
      "panes":[{"endpoint_id":"ep_a","generation":"gen_a","session_id":"$1","window_id":"@1","pane_id":"%%1","index":"0","active":true,"activity":"","tty":"%s","path":"/tmp","command":"zsh","title":"inner"}],
      "clients":[{"endpoint_id":"ep_a","generation":"gen_a","client_name":"%s","client_pid":"10","client_created":"11","client_tty":"%s","client_uid":"501","attached_session_id":"$1","current_window_id":"@1","current_pane_id":"%%1"}]
    }] }]], client_tty, client_tty, client_tty)
end

local function supervised_child(args)
  for index, value in ipairs(args) do
    if value == '--' then
      local child = {}
      for child_index=index+1,#args do child[#child+1] = args[child_index] end
      return child
    end
  end
  error('missing supervisor separator')
end

local function flag_value(args, wanted)
  for index, value in ipairs(args) do if value == wanted then return args[index+1] end end
end

local function inventory_for_request(tty, child)
  return inventory_json(tty):gsub('"request_id":"r"', '"request_id":"' .. flag_value(child, '--request-id') .. '"')
end

local function fake_wezterm(windows, runner)
  local logs = {}
  local wezterm = {
    mux={all_windows=function() return windows end},
    gui={gui_window_for_mux_window=function() return {focus=function() end} end},
    run_child_process=runner,
    action={},
    action_callback=function(fn) return fn end,
    log_error=function(message) logs[#logs + 1] = 'error:' .. message end,
    log_warn=function(message) logs[#logs + 1] = 'warn:' .. message end,
  }
  wezterm.action.InputSelector=function(spec) return {kind='selector',spec=spec} end
  wezterm.action.SwitchToWorkspace=function(spec) return {kind='workspace',spec=spec} end
  wezterm.action.SpawnCommandInNewTab=function(spec) return {kind='spawn',spec=spec} end
  return wezterm, logs
end

local function gui_window()
  return {
    actions={},
    perform_action=function(self, action, active_pane)
      self.actions[#self.actions + 1] = {action=action,pane=active_pane}
    end,
  }
end

local function find_choice(selector, pattern)
  for _, choice in ipairs(selector.spec.choices) do if choice.label:match(pattern) then return choice.id end end
  error('choice not found: ' .. pattern)
end

function suite.remote_invocation_never_runs_local_tmux_or_spawns_attachment()
  local p, mux = mux_fixture()
  p.get_domain_name = function() return 'ssh:host' end
  local wezterm = fake_wezterm({mux}, function() error('tmx must not run from remote domain') end)
  local switcher = adapter.new(wezterm, {enabled=true,allowed_local_domains={'local'}})
  local window = gui_window()
  switcher.open(window, p, true)
  local selector = window.actions[1].action
  assert(#selector.spec.choices == 2)
  assert(selector.spec.title:match('non%-local mux domain'))
  selector.spec.action(window, p, nil, nil)
end

function suite.route_response_requires_matching_schema_request_plan_and_fields()
  local cases = {'wrong-schema','wrong-request','wrong-plan','missing-field'}
  for _, case in ipairs(cases) do
    local p, mux = mux_fixture()
    local wezterm, logs = fake_wezterm({mux}, function(args)
      local child = supervised_child(args)
      if child[2] == 'inventory' then return true, inventory_for_request('/dev/ttys001', child), '' end
      local request = flag_value(child, '--request-id')
      local schema = case == 'wrong-schema' and 2 or 1
      local response_request = case == 'wrong-request' and 'stale' or request
      local plan = case == 'wrong-plan' and 'new_attachment' or 'mapped_client'
      local elapsed = case == 'missing-field' and '' or ',"elapsed_ms":1'
      return true, '{"schema":{"name":"dev.tmx.route","major":' .. schema .. ',"minor":0},"request_id":"' .. response_request .. '","plan_kind":"' .. plan .. '","outcome":"success"' .. elapsed .. ',"diagnostics":[]}', ''
    end)
    local switcher = adapter.new(wezterm, {enabled=true})
    local window = gui_window()
    switcher.open(window, p, true)
    local selector = window.actions[1].action
    selector.spec.action(window, p, find_choice(selector, '%[tmux pane%]'), '')
    assert(#logs == 1 and logs[1]:match('tmx switcher'))
    assert(#window.actions == 1)
  end
end

function suite.native_rows_survive_malformed_oversized_incompatible_and_timeout_inventory()
  local cases = {'malformed','oversized','incompatible','timeout'}
  for _, case in ipairs(cases) do
    local p, mux = mux_fixture()
    local wezterm = fake_wezterm({mux}, function(args)
      local child = supervised_child(args)
      if case == 'timeout' then return false, '', 'deadline' end
      if case == 'malformed' then return true, '{', '' end
      if case == 'oversized' then return true, string.rep('x', 8 * 1024 * 1024 + 1), '' end
      local body = inventory_for_request('/dev/ttys001', child)
      return true, body:gsub('"major":1', '"major":2', 1), ''
    end)
    local switcher = adapter.new(wezterm, {enabled=true})
    local window = gui_window()
    switcher.open(window, p, true)
    local selector = window.actions[1].action
    assert(#selector.spec.choices == 2, case)
    selector.spec.action(window, p, nil, nil)
  end
end

function suite.missing_tmx_preserves_native_selector_and_cancel_is_noop()
  local p, mux = mux_fixture()
  local wezterm, logs = fake_wezterm({mux}, function()
    return false, '', 'error connecting to /Users/private/secret.sock'
  end)
  local switcher = adapter.new(wezterm, {enabled=true})
  local window = gui_window()
  switcher.open(window, p, true)
  assert(#window.actions == 1)
  local selector = window.actions[1].action
  assert(selector.kind == 'selector' and selector.spec.fuzzy == true)
  assert(#selector.spec.choices == 2)
  selector.spec.action(window, p, nil, nil)
  assert(#window.actions == 1)
  assert(#logs == 0)
end

function suite.repeat_invocation_does_not_stack_selectors()
  local p, mux = mux_fixture()
  local wezterm = fake_wezterm({mux}, function() return false, '', 'missing' end)
  local switcher = adapter.new(wezterm, {enabled=true})
  local window = gui_window()
  switcher.open(window, p, true)
  switcher.open(window, p, true)
  assert(#window.actions == 1)
  window.actions[1].action.spec.action(window, p, nil, nil)
  switcher.open(window, p, false)
  assert(#window.actions == 2)
  assert(window.actions[2].action.spec.fuzzy == false)
  window.actions[2].action.spec.action(window, p, nil, nil)
end

function suite.unmatched_tmux_target_spawns_one_validated_local_domain_tab()
  local p, mux = mux_fixture()
  p.get_domain_name = function() return 'unix:acceptance' end
  local wezterm = fake_wezterm({mux}, function(args)
    local child = supervised_child(args)
    if child[2] == 'inventory' then return true, inventory_for_request('/dev/ttys999', child), '' end
    error('unexpected child process')
  end)
  local switcher = adapter.new(wezterm, {enabled=true,allowed_local_domains={'unix:acceptance'}})
  local window = gui_window()
  switcher.open(window, p, true)
  local selector = window.actions[1].action
  selector.spec.action(window, p, find_choice(selector, '%[tmux session%]'), '')
  assert(#window.actions == 2)
  local spawn = window.actions[2].action
  assert(spawn.kind == 'spawn')
  assert(spawn.spec.domain == 'unix:acceptance')
  assert(spawn.spec.args[1] == 'tmx' and spawn.spec.args[2] == 'attach')
  local joined = table.concat(spawn.spec.args, ' ')
  assert(joined:match('%-%-endpoint%-id ep_a'))
  assert(not joined:match('sh %-c'))
end

function suite.new_attachment_revalidates_invoking_domain_before_spawn()
  local p, mux = mux_fixture()
  local wezterm, logs = fake_wezterm({mux}, function(args)
    local child = supervised_child(args)
    if child[2] == 'inventory' then return true, inventory_for_request('/dev/ttys999', child), '' end
    error('unexpected child process')
  end)
  local switcher = adapter.new(wezterm, {enabled=true,allowed_local_domains={'local'}})
  local window = gui_window()
  switcher.open(window, p, true)
  local selector = window.actions[1].action
  p.get_domain_name = function() return 'ssh:remote-default' end
  selector.spec.action(window, p, find_choice(selector, '%[tmux session%]'), '')
  assert(#window.actions == 1)
  assert(logs[#logs]:match('no longer in an allowed local mux domain'))
end

function suite.matched_client_uses_typed_route_then_focuses_host()
  local p, mux = mux_fixture()
  local calls = {}
  local wezterm = fake_wezterm({mux}, function(args)
    calls[#calls + 1] = args
    local child = supervised_child(args)
    if child[2] == 'inventory' then return true, inventory_for_request('/dev/ttys001', child), '' end
    if child[2] == 'route' then
      local request_id = flag_value(child, '--request-id')
      return true, '{"schema":{"name":"dev.tmx.route","major":1,"minor":0},"request_id":"' .. request_id .. '","plan_kind":"mapped_client","outcome":"success","elapsed_ms":1,"diagnostics":[]}', ''
    end
    error('unexpected command')
  end)
  local switcher = adapter.new(wezterm, {enabled=true})
  local window = gui_window()
  switcher.open(window, p, true)
  local selector = window.actions[1].action
  selector.spec.action(window, p, find_choice(selector, '%[tmux pane%]'), '')
  assert(#calls == 2)
  assert(calls[1][1] == 'tmx-supervisor' and calls[2][1] == 'tmx-supervisor')
  local routed = supervised_child(calls[2])
  assert(routed[2] == 'route')
  local joined = table.concat(routed, ' ')
  assert(joined:match('%-%-client%-name /dev/ttys001'))
  assert(joined:match('%-%-pane%-id %%1'))
  assert(#window.actions == 1)
end

function suite.inactive_workspace_selection_activates_exact_pane_and_focuses_gui()
  local p, mux = mux_fixture()
  local other, other_mux = mux_fixture(200, '/dev/ttys002', 'other', 2, 20)
  local wezterm = fake_wezterm({mux, other_mux}, function() return false, '', 'missing' end)
  local focused = false
  wezterm.gui.gui_window_for_mux_window = function(id)
    assert(id == 2)
    return {focus=function() focused = true end}
  end
  local switcher = adapter.new(wezterm, {enabled=true})
  local window = gui_window()
  switcher.open(window, p, true)
  local selector = window.actions[1].action
  selector.spec.action(window, p, find_choice(selector, 'pane%-200'), '')
  assert(other.activated == true)
  assert(focused == true)
  assert(window.actions[2].action.kind == 'workspace')
  assert(window.actions[2].action.spec.name == 'other')
end

function suite.focus_failure_after_activation_is_partial_and_not_retried()
  local p, mux = mux_fixture()
  local other, other_mux = mux_fixture(200, '/dev/ttys002', 'other', 2, 20)
  local wezterm, logs = fake_wezterm({mux, other_mux}, function() error('no subprocess expected') end)
  wezterm.gui.gui_window_for_mux_window = function()
    return {focus=function() error('focus denied') end}
  end
  local switcher = adapter.new(wezterm, {enabled=false})
  local window = gui_window()
  switcher.open(window, p, true)
  local selector = window.actions[1].action
  selector.spec.action(window, p, find_choice(selector, 'pane%-200'), '')
  assert(other.activated == true)
  assert(#logs == 1 and logs[1]:match('routed_but_not_focused'))
end

function suite.stale_native_selection_reports_error_without_creating_workspace()
  local p, mux = mux_fixture()
  local _, other_mux = mux_fixture(200, '/dev/ttys002', 'other', 2, 20)
  local windows = {mux, other_mux}
  local wezterm, logs = fake_wezterm(windows, function() return false, '', 'missing' end)
  local switcher = adapter.new(wezterm, {enabled=true})
  local window = gui_window()
  switcher.open(window, p, true)
  local selector = window.actions[1].action
  windows[2] = nil
  selector.spec.action(window, p, find_choice(selector, 'pane%-200'), '')
  assert(#window.actions == 1)
  assert(logs[1]:match('stale native destination'))
end

function suite.old_wezterm_capability_gap_preserves_existing_bindings()
  local wezterm = {action={},mux={all_windows=function() return {} end},log_warn=function() end}
  local config = {keys={{key='9',mods='ALT',action='old'}}}
  local installed = adapter.apply_to_config(config, wezterm, {enabled=true})
  assert(installed == nil)
  assert(#config.keys == 1 and config.keys[1].action == 'old')
end

function suite.apply_preserves_unrelated_keys_and_installs_both_modes_and_emergency()
  local p, mux = mux_fixture()
  local wezterm = fake_wezterm({mux}, function() return false, '', 'missing' end)
  local config = {keys={{key='x',mods='CTRL',action='keep'},{key='9',mods='ALT',action='old'}}}
  adapter.apply_to_config(config, wezterm, {enabled=true})
  assert(#config.keys == 4)
  assert(config.keys[1].key == 'x')
  assert(config.keys[2].key == 'phys:9' and config.keys[2].mods == 'ALT')
  assert(config.keys[3].key == 'phys:9' and config.keys[3].mods == 'ALT|SHIFT')
  assert(config.keys[4].key == 'phys:0' and config.keys[4].mods == 'ALT')
  assert(p and mux)
end

return suite
