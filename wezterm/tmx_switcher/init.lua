local model = require 'tmx_switcher.model'
local strict_json = require 'tmx_switcher.json'

local M = {}
local invocation_active = false

local function call(object, method, ...)
  if object == nil or type(object[method]) ~= 'function' then return nil end
  local ok, value = pcall(object[method], object, ...)
  if ok then return value end
  return nil
end

local function text(value)
  if value == nil then return '' end
  return tostring(value)
end

local function cwd_text(value)
  if value == nil then return '' end
  if type(value) == 'table' and type(value.file_path) == 'string' then return value.file_path end
  local result = tostring(value)
  result = result:gsub('^file://[^/]*', '')
  return result
end

local function capability(inventory, wanted)
  if type(inventory) ~= 'table' or type(inventory.capabilities) ~= 'table' then return false end
  for _, value in ipairs(inventory.capabilities) do if value == wanted then return true end end
  return false
end

local function dense_array(value)
  if type(value) ~= 'table' then return false end
  local mt = getmetatable(value)
  if mt and mt.__tmx_json_kind == 'object' then return false end
  local count, maximum = 0, 0
  for key in pairs(value) do
    if type(key) ~= 'number' or key < 1 or key % 1 ~= 0 then return false end
    count, maximum = count + 1, math.max(maximum, key)
  end
  return count == maximum
end

local function validate_route_response(response, request_id)
  if type(response) ~= 'table' or type(response.schema) ~= 'table'
      or response.schema.name ~= 'dev.tmx.route' or response.schema.major ~= 1
      or type(response.schema.minor) ~= 'number' then
    return false, 'incompatible route response schema'
  end
  if response.request_id ~= request_id or response.plan_kind ~= 'mapped_client'
      or type(response.outcome) ~= 'string' or type(response.elapsed_ms) ~= 'number'
      or response.elapsed_ms < 0 or response.elapsed_ms ~= response.elapsed_ms or response.elapsed_ms == math.huge
      or not dense_array(response.diagnostics) then
    return false, 'route response identity or required fields are invalid'
  end
  return true
end

local function target_kind_args(route)
  local args = {'--target-kind', route.kind, '--session-id', route.session_id}
  if route.window_id then
    args[#args + 1] = '--window-id'; args[#args + 1] = route.window_id
  end
  if route.pane_id then
    args[#args + 1] = '--pane-id'; args[#args + 1] = route.pane_id
  end
  return args
end

function M.new(wezterm, options)
  assert(type(wezterm) == 'table', 'tmx_switcher requires the wezterm module')
  options = options or {}
  local tmx_bin = options.tmx_bin or 'tmx'
  local supervisor_bin = options.supervisor_bin
  if not supervisor_bin then
    supervisor_bin = tmx_bin:match('^(.*[/\\])tmx$')
    supervisor_bin = supervisor_bin and (supervisor_bin .. 'tmx-supervisor') or 'tmx-supervisor'
  end
  local inventory_deadline_ms = math.min(math.max(100, options.inventory_deadline_ms or 400), 2000)
  local route_deadline_ms = math.min(math.max(25, options.route_deadline_ms or 250), 2000)
  local enabled = options.enabled ~= false
  local focus_history, focus_counter = {}, 0

  local function is_allowed_local_domain(domain)
    for _, allowed in ipairs(options.allowed_local_domains or {'local'}) do
      if domain == allowed then return true end
    end
    return false
  end

  local function all_windows()
    if options.all_windows then return options.all_windows() end
    return wezterm.mux.all_windows()
  end

  local function native_snapshot(invoking_pane)
    local invoking_id = text(call(invoking_pane, 'pane_id'))
    local rows, current_workspace, current_mux_window, current_domain = {}, '', nil, ''
    local snapshot_cap = math.min(math.max(1, tonumber(options.max_rows) or 10000), 10000) + 1
    local function push(item)
      if #rows < snapshot_cap or item.current then rows[#rows + 1] = item end
    end
    for _, mux_window in ipairs(all_windows() or {}) do
      local mux_window_id = text(call(mux_window, 'window_id'))
      local workspace = text(call(mux_window, 'get_workspace'))
      for _, tab in ipairs(call(mux_window, 'tabs') or {}) do
        local tab_id = text(call(tab, 'tab_id'))
        local active = call(tab, 'active_pane')
        local active_id = text(call(active, 'pane_id'))
        local function pane_fields(pane)
          return {
            domain=text(call(pane, 'get_domain_name')),
            workspace=workspace,
            mux_window_id=mux_window_id,
            tab_id=tab_id,
            pane_id=text(call(pane, 'pane_id')),
            title=text(call(pane, 'get_title')),
            path=cwd_text(call(pane, 'get_current_working_dir')),
            command=text(call(pane, 'get_foreground_process_name')),
            tty=text(call(pane, 'get_tty_name')),
            current=text(call(pane, 'pane_id')) == invoking_id,
          }
        end
        local tab_fields = pane_fields(active)
        tab_fields.kind = 'tab'
        local tab_title = text(call(tab, 'get_title'))
        if tab_title ~= '' then tab_fields.title = tab_title end
        push(tab_fields)
        for _, pane in ipairs(call(tab, 'panes') or {}) do
          local fields = pane_fields(pane)
          fields.kind = 'pane'
          push(fields)
          if fields.current then
            current_workspace = workspace
            current_mux_window = mux_window_id
            current_domain = fields.domain
          end
        end
      end
    end
    if current_mux_window then
      focus_counter = focus_counter + 1
      focus_history[current_mux_window] = focus_counter
    end
    for _, item in ipairs(rows) do
      item.focus_rank = focus_history[item.mux_window_id] or -1
    end
    return rows, current_workspace, current_domain
  end

  local function resolve_native(identity)
    for _, mux_window in ipairs(all_windows() or {}) do
      if text(call(mux_window, 'window_id')) == identity.mux_window_id
          and text(call(mux_window, 'get_workspace')) == identity.workspace then
        for _, tab in ipairs(call(mux_window, 'tabs') or {}) do
          if text(call(tab, 'tab_id')) == identity.tab_id then
            for _, pane in ipairs(call(tab, 'panes') or {}) do
              if text(call(pane, 'pane_id')) == identity.pane_id
                  and text(call(pane, 'get_domain_name')) == identity.domain then
                return mux_window, pane
              end
            end
          end
        end
      end
    end
    return nil, nil
  end

  local function focus_native(window, pane, route)
    if route.current then return true, 'no_op' end
    local mux_window, target = resolve_native(route.identity)
    if not target then return false, 'stale native destination' end
    -- SwitchToWorkspace can create an absent workspace, so use it only after a
    -- fresh mux snapshot proved this exact workspace/window/pane identity.
    if route.identity.workspace ~= '' then
      pcall(window.perform_action, window, wezterm.action.SwitchToWorkspace {name=route.identity.workspace}, pane)
    end
    local activated = pcall(target.activate, target)
    if not activated then return false, 'pane activation failed' end
    local gui = wezterm.gui and wezterm.gui.gui_window_for_mux_window(
      tonumber(route.identity.mux_window_id) or route.identity.mux_window_id
    ) or nil
    if gui and type(gui.focus) == 'function' then
      local focused = pcall(gui.focus, gui)
      if not focused then return true, 'routed_but_not_focused' end
    end
    return true, gui and 'routed_and_focused' or 'routed_without_gui'
  end

  local function supervised(child_args, deadline_ms, stdout_limit)
    local args = {
      supervisor_bin, '--deadline-ms', tostring(math.min(2000, deadline_ms + 50)),
      '--stdout-limit', tostring(stdout_limit), '--stderr-limit', '16384', '--',
    }
    for _, value in ipairs(child_args) do args[#args + 1] = value end
    return args
  end

  local function inventory(snapshot)
    if not enabled then return nil, 'tmux augmentation disabled locally' end
    local request = string.format('wezterm-%d-%d', os.time(), #snapshot)
    local runner = options.run_child_process or wezterm.run_child_process
    local child_args = {
      tmx_bin, 'inventory', '--schema', '1', '--json',
      '--request-id', request, '--deadline-ms', tostring(inventory_deadline_ms),
    }
    local ok, stdout = runner(supervised(child_args, inventory_deadline_ms, 8 * 1024 * 1024))
    if not ok then return nil, 'tmx inventory unavailable' end
    local decoded_ok, decoded = pcall(strict_json.decode, stdout, {
      max_bytes=8 * 1024 * 1024, max_depth=16, max_nodes=50000,
    })
    if not decoded_ok then return nil, 'tmx inventory malformed: ' .. model.sanitize(decoded, 120) end
    if type(decoded) ~= 'table' or decoded.request_id ~= request then
      return nil, 'tmx inventory response identity mismatch'
    end
    if not capability(decoded, 'augmentation_enabled') then
      return nil, 'tmux augmentation is disabled in tmx config'
    end
    return decoded
  end

  local function route_tmux(window, pane, route)
    local request = string.format('wezterm-route-%d', os.time())
    if route.client then
      local args = {
        tmx_bin, 'route', '--schema', '1', '--json', '--request-id', request,
        '--host-domain', route.host_domain, '--endpoint-id', route.endpoint_id,
        '--generation', route.generation,
      }
      for _, value in ipairs(target_kind_args(route)) do args[#args + 1] = value end
      for _, pair in ipairs({
        {'--mode', 'prefer-client'}, {'--client-name', route.client.client_name},
        {'--client-tty', route.client.client_tty}, {'--client-pid', route.client.client_pid},
        {'--client-created', route.client.client_created}, {'--client-uid', route.client.client_uid},
        {'--deadline-ms', tostring(route_deadline_ms)},
      }) do args[#args + 1] = pair[1]; args[#args + 1] = pair[2] end
      local runner = options.run_child_process or wezterm.run_child_process
      local ok, stdout = runner(supervised(args, route_deadline_ms, 65536))
      if not ok then return false, 'tmx route unavailable' end
      local parsed_ok, response = pcall(strict_json.decode, stdout, {max_bytes=65536, max_depth=16, max_nodes=1000})
      if not parsed_ok then return false, 'tmx route returned malformed JSON' end
      local response_valid, response_error = validate_route_response(response, request)
      if not response_valid then return false, response_error end
      if response.outcome ~= 'success' and response.outcome ~= 'partial_success' then
        return false, 'tmx route outcome: ' .. model.sanitize(response.outcome, 80)
      end
      if route.host_pane then
        local host_route = {identity={
          domain=text(route.host_pane.domain), workspace=text(route.host_pane.workspace),
          mux_window_id=text(route.host_pane.mux_window_id), tab_id=text(route.host_pane.tab_id),
          pane_id=text(route.host_pane.pane_id)
        }, current=route.host_pane.current == true}
        local focused, detail = focus_native(window, pane, host_route)
        if not focused then return true, 'routed_but_not_focused: ' .. detail end
      end
      return true, response.outcome
    end

    local spawn_domain = text(call(pane, 'get_domain_name'))
    if spawn_domain == '' or not is_allowed_local_domain(spawn_domain) then
      return false, 'invoking pane is no longer in an allowed local mux domain'
    end
    local args = {
      tmx_bin, 'attach', '--schema', '1', '--request-id', request,
      '--host-domain', route.host_domain, '--endpoint-id', route.endpoint_id,
      '--generation', route.generation,
    }
    for _, value in ipairs(target_kind_args(route)) do args[#args + 1] = value end
    args[#args + 1] = '--deadline-ms'; args[#args + 1] = tostring(route_deadline_ms)
    window:perform_action(wezterm.action.SpawnCommandInNewTab {
      args=args,
      domain=spawn_domain,
    }, pane)
    return true, 'success_new_attachment'
  end

  local function open(window, pane, fuzzy, native_only)
    if invocation_active then return end
    invocation_active = true
    local snapshot, current_workspace, current_domain = native_snapshot(pane)
    local tmux_inventory, inventory_status
    local domain_allowed = is_allowed_local_domain(current_domain)
    if not native_only and domain_allowed then
      tmux_inventory, inventory_status = inventory(snapshot)
    elseif not native_only and not domain_allowed then
      inventory_status = 'tmux augmentation unavailable from non-local mux domain'
    end
    local built = model.build(snapshot, tmux_inventory, {
      current_workspace=current_workspace,
      allowed_local_domains=options.allowed_local_domains or {'local'},
      max_rows=options.max_rows or 10000,
    })
    if inventory_status then table.insert(built.diagnostics, inventory_status) end
    local status = #built.diagnostics > 0 and (' — ' .. model.sanitize(built.diagnostics[1], 100)) or ''
    local action = wezterm.action.InputSelector {
      title='Destinations' .. status,
      choices=built.choices,
      fuzzy=fuzzy,
      fuzzy_description='Type to filter · Enter accept · Esc/Ctrl-C/Ctrl-G cancel',
      description='Jump key or arrows · Enter accept · Esc/Ctrl-C/Ctrl-G cancel',
      action=wezterm.action_callback(function(inner_window, inner_pane, id, _label)
        invocation_active = false
        if not id then return end
        local selected = built.routes[id]
        if not selected then return end
        local ok, result
        if selected.provider == 'wezterm' then
          ok, result = focus_native(inner_window, inner_pane, selected)
        else
          ok, result = route_tmux(inner_window, inner_pane, selected)
        end
        if not ok then
          wezterm.log_error('tmx switcher: ' .. model.sanitize(result, 200))
        elseif result == 'routed_but_not_focused' or result == 'partial_success' then
          wezterm.log_warn('tmx switcher: ' .. result)
        end
      end),
    }
    local presented, err = pcall(window.perform_action, window, action, pane)
    if not presented then
      invocation_active = false
      wezterm.log_error('tmx switcher: selector failed: ' .. model.sanitize(err, 160))
    end
  end

  return {
    open=function(window, pane, fuzzy) return open(window, pane, fuzzy, false) end,
    open_native=function(window, pane, fuzzy) return open(window, pane, fuzzy, true) end,
    snapshot=native_snapshot,
    resolve_native=resolve_native,
  }
end

function M.apply_to_config(config, wezterm, options)
  if not wezterm.action or wezterm.action.InputSelector == nil
      or not wezterm.mux or type(wezterm.mux.all_windows) ~= 'function' then
    if type(wezterm.log_warn) == 'function' then
      wezterm.log_warn('tmx switcher: this WezTerm build lacks InputSelector or global mux enumeration; existing bindings were preserved')
    end
    return nil
  end
  local switcher = M.new(wezterm, options)
  local keys = {}
  for _, binding in ipairs(config.keys or {}) do
    local is_nine = binding.key == '9' or binding.key == 'phys:9'
    local is_owned = is_nine and (binding.mods == 'ALT' or binding.mods == 'ALT|SHIFT')
    if not is_owned then keys[#keys + 1] = binding end
  end
  keys[#keys + 1] = {key='phys:9', mods='ALT', action=wezterm.action_callback(function(w, p) switcher.open(w, p, true) end)}
  keys[#keys + 1] = {key='phys:9', mods='ALT|SHIFT', action=wezterm.action_callback(function(w, p) switcher.open(w, p, false) end)}
  local native_key = (options and options.native_only_key) or 'phys:0'
  keys[#keys + 1] = {key=native_key, mods='ALT', action=wezterm.action_callback(function(w, p) switcher.open_native(w, p, true) end)}
  config.keys = keys
  return switcher
end

return M
