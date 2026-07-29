local M = {}

local MAX_LABEL = 512
local MAX_METADATA = 4096
local KNOWN_STATUS = { available=true, partial=true, unavailable_endpoint=true, untrusted_endpoint=true, incompatible=true, timeout=true }

local function truncate(value, limit)
  if #value <= limit then return value end
  if limit <= 3 then return string.rep(".", math.max(0, limit)) end
  local budget, index, last = limit - 3, 1, 0
  while index <= #value do
    local byte = value:byte(index)
    local width = byte < 0x80 and 1 or (byte < 0xe0 and 2 or (byte < 0xf0 and 3 or 4))
    if index + width - 1 > budget then break end
    last = index + width - 1
    index = index + width
  end
  return value:sub(1, last) .. "…"
end

function M.sanitize(value, limit)
  if value == nil then return "" end
  value = tostring(value)
  value = value:gsub("\27%[[0-9;?]*[ -/]*[@-~]", "")
  value = value:gsub("[%z\1-\31\127]", " ")
  value = value:gsub("%s+", " "):match("^%s*(.-)%s*$")
  return truncate(value, limit or MAX_METADATA)
end

local function required_string(value, field, allow_empty)
  if type(value) ~= "string" or (not allow_empty and value == "") or #value > MAX_METADATA then
    return nil, field .. " is missing, not a string, or outside its limit"
  end
  return value
end

local function required_array(value, field)
  if type(value) ~= "table" then return nil, field .. " is not an array" end
  local mt = getmetatable(value)
  if mt and mt.__tmx_json_kind == "object" then return nil, field .. " is not an array" end
  local count, maximum = 0, 0
  for key in pairs(value) do
    if type(key) ~= "number" or key < 1 or key % 1 ~= 0 then return nil, field .. " is not an array" end
    count, maximum = count + 1, math.max(maximum, key)
  end
  if count ~= maximum then return nil, field .. " is sparse" end
  return value
end

local function has_sigil(value, sigil)
  return type(value) == "string" and value:sub(1, #sigil) == sigil and #value > #sigil
end

local function decimal(value, allow_empty)
  return type(value) == "string" and ((allow_empty and value == "") or (#value <= 20 and value:match("^%d+$") ~= nil))
end

local function validate_inventory(inventory)
  if type(inventory) ~= "table" then return nil, "inventory is not an object" end
  local schema = inventory.schema
  if type(schema) ~= "table" or schema.name ~= "dev.tmx.inventory" or schema.major ~= 1 then
    return nil, "inventory schema is incompatible"
  end
  if type(inventory.capabilities) ~= "table" or type(inventory.endpoints) ~= "table" then
    return nil, "inventory envelope is missing capabilities or endpoints"
  end
  if #inventory.endpoints > 32 then return nil, "inventory endpoint limit exceeded" end
  local targets, endpoint_ids = 0, {}
  for ei, endpoint in ipairs(inventory.endpoints) do
    if type(endpoint) ~= "table" then return nil, "endpoint record is not an object" end
    local ok, err = required_string(endpoint.host_domain, "host_domain")
    if not ok or endpoint.host_domain ~= "local" then return nil, "endpoint " .. ei .. ": unsupported host_domain" end
    ok, err = required_string(endpoint.endpoint_id, "endpoint_id")
    if not ok then return nil, "endpoint " .. ei .. ": " .. err end
    if not has_sigil(endpoint.endpoint_id, "ep_") or endpoint_ids[endpoint.endpoint_id] then
      return nil, "endpoint identity is duplicate or malformed"
    end
    endpoint_ids[endpoint.endpoint_id] = true
    ok, err = required_string(endpoint.alias, "alias")
    if not ok then return nil, "endpoint " .. ei .. ": " .. err end
    if not KNOWN_STATUS[endpoint.status] then
      endpoint._unknown_status = true
    end
    for _, field in ipairs({"sessions", "windows", "panes", "clients"}) do
      ok, err = required_array(endpoint[field], field)
      if not ok then return nil, "endpoint " .. ei .. ": " .. err end
    end
    if not endpoint._unknown_status and (endpoint.status == "available" or endpoint.status == "partial") then
      if type(endpoint.generation) ~= "table" then return nil, "available endpoint lacks generation" end
      ok, err = required_string(endpoint.generation.token, "generation.token")
      if not ok or not has_sigil(endpoint.generation.token, "gen_") then return nil, err or "generation token is malformed" end
      local sessions, windows, panes, clients = {}, {}, {}, {}
      local function same_endpoint(record)
        return record.endpoint_id == endpoint.endpoint_id and record.generation == endpoint.generation.token
      end
      for _, session in ipairs(endpoint.sessions) do
        if type(session) ~= "table" then return nil, "session record is not an object" end
        for _, field in ipairs({"endpoint_id", "generation", "session_id", "name", "path", "created", "activity", "attached_count", "window_count"}) do
          ok, err = required_string(session[field], "session." .. field, field == "path")
          if not ok then return nil, err end
        end
        if not same_endpoint(session) or not has_sigil(session.session_id, "$") or sessions[session.session_id]
            or not decimal(session.created) or not decimal(session.activity)
            or not decimal(session.attached_count) or not decimal(session.window_count) then
          return nil, "session identity is duplicate, malformed, or outside endpoint generation"
        end
        sessions[session.session_id] = session
      end
      for _, window in ipairs(endpoint.windows) do
        if type(window) ~= "table" then return nil, "window record is not an object" end
        for _, field in ipairs({"endpoint_id", "generation", "session_id", "window_id", "index", "name", "activity"}) do
          ok, err = required_string(window[field], "window." .. field, field == "activity")
          if not ok then return nil, err end
        end
        local key = window.session_id .. "\31" .. window.window_id
        if not same_endpoint(window) or not has_sigil(window.session_id, "$") or not has_sigil(window.window_id, "@")
            or not sessions[window.session_id] or windows[key]
            or not decimal(window.index) or not decimal(window.activity, true) then
          return nil, "window identity is duplicate, malformed, or has no session parent"
        end
        if type(window.active) ~= "boolean" then return nil, "window.active is not boolean" end
        windows[key] = window
      end
      for _, pane in ipairs(endpoint.panes) do
        if type(pane) ~= "table" then return nil, "pane record is not an object" end
        for _, field in ipairs({"endpoint_id", "generation", "session_id", "window_id", "pane_id", "index", "activity"}) do
          ok, err = required_string(pane[field], "pane." .. field, field == "activity")
          if not ok then return nil, err end
        end
        local window_key = pane.session_id .. "\31" .. pane.window_id
        local key = window_key .. "\31" .. pane.pane_id
        if not same_endpoint(pane) or not has_sigil(pane.session_id, "$") or not has_sigil(pane.window_id, "@")
            or not has_sigil(pane.pane_id, "%") or not windows[window_key] or panes[key]
            or not decimal(pane.index) or not decimal(pane.activity, true) then
          return nil, "pane identity is duplicate, malformed, or has no window parent"
        end
        if type(pane.active) ~= "boolean" then return nil, "pane.active is not boolean" end
        panes[key] = pane
      end
      for _, client in ipairs(endpoint.clients) do
        if type(client) ~= "table" then return nil, "client record is not an object" end
        for _, field in ipairs({"endpoint_id", "generation", "client_name", "client_pid", "client_created", "client_tty", "client_uid", "attached_session_id"}) do
          ok, err = required_string(client[field], "client." .. field, field == "client_tty")
          if not ok then return nil, err end
        end
        local key = table.concat({client.client_name, client.client_pid, client.client_created, client.client_uid}, "\31")
        if not same_endpoint(client) or not has_sigil(client.attached_session_id, "$")
            or not sessions[client.attached_session_id] or clients[key]
            or not decimal(client.client_pid) or not decimal(client.client_created) or not decimal(client.client_uid) then
          return nil, "client identity is duplicate, malformed, or has no session parent"
        end
        if client.current_window_id ~= nil then
          ok, err = required_string(client.current_window_id, "client.current_window_id", true)
          if not ok then return nil, err end
          if client.current_window_id ~= "" and not windows[client.attached_session_id .. "\31" .. client.current_window_id] then
            return nil, "client current window has no parent"
          end
        end
        if client.current_pane_id ~= nil then
          ok, err = required_string(client.current_pane_id, "client.current_pane_id", true)
          if not ok then return nil, err end
          local pane_key = client.attached_session_id .. "\31" .. tostring(client.current_window_id) .. "\31" .. client.current_pane_id
          if client.current_pane_id ~= "" and not panes[pane_key] then return nil, "client current pane has no parent" end
        end
        clients[key] = client
      end
      targets = targets + #endpoint.sessions + #endpoint.windows + #endpoint.panes
      if targets > 10000 then return nil, "inventory target limit exceeded" end
    end
  end
  return inventory
end

local function identity_native(item)
  return table.concat({item.domain or "", item.mux_window_id or "", item.tab_id or "", item.pane_id or "", item.kind or ""}, "\31")
end

local function basename(path)
  path = M.sanitize(path)
  return path:match("([^/]+)/*$") or path
end

local function activity(value)
  local number = tonumber(value or "")
  if not number or number ~= number or number == math.huge then return -1 end
  return number
end

local function tty(value)
  value = M.sanitize(value)
  if value == "" then return nil end
  return value:gsub("/+", "/")
end

local function local_domains(options)
  local out = {}
  for _, name in ipairs(options.allowed_local_domains or {"local"}) do out[name] = true end
  return out
end

local function choose_client_candidates(native, endpoint, options)
  local allowed = local_domains(options)
  local by_tty = {}
  for _, item in ipairs(native) do
    local normalized = tty(item.tty)
    if item.kind == "pane" and normalized and allowed[item.domain] then
      by_tty[normalized] = by_tty[normalized] or {}
      table.insert(by_tty[normalized], item)
    end
  end
  local clients_by_tty = {}
  for _, client in ipairs(endpoint.clients) do
    local normalized = tty(client.client_tty)
    if normalized then
      clients_by_tty[normalized] = clients_by_tty[normalized] or {}
      table.insert(clients_by_tty[normalized], client)
    end
  end
  local candidates = {}
  for normalized, clients in pairs(clients_by_tty) do
    local panes = by_tty[normalized]
    if #clients == 1 and panes and #panes == 1 then
      table.insert(candidates, { client=clients[1], pane=panes[1] })
    end
  end
  table.sort(candidates, function(a, b)
    local ai = a.pane.current and 1 or 0
    local bi = b.pane.current and 1 or 0
    if ai ~= bi then return ai > bi end
    local aw = a.pane.workspace == options.current_workspace and 1 or 0
    local bw = b.pane.workspace == options.current_workspace and 1 or 0
    if aw ~= bw then return aw > bw end
    local af = tonumber(a.pane.focus_rank) or -1
    local bf = tonumber(b.pane.focus_rank) or -1
    if af ~= bf then return af > bf end
    return identity_native(a.pane) < identity_native(b.pane)
  end)
  return candidates[1]
end

local function client_at(client, kind, session_id, window_id, pane_id)
  if client.attached_session_id ~= session_id then return false end
  if kind == "session" then return true end
  if client.current_window_id ~= window_id then return false end
  return kind == "window" or client.current_pane_id == pane_id
end

local function add_native(rows, native, options)
  for _, item in ipairs(native) do
    if item.kind == "tab" or item.kind == "pane" then
      local title = M.sanitize(item.title)
      if title == "" then title = item.kind == "tab" and "untitled tab" or "untitled pane" end
      local context = {}
      for _, value in ipairs({item.workspace, basename(item.path), item.command}) do
        value = M.sanitize(value)
        if value ~= "" then table.insert(context, value) end
      end
      local state = item.current and " [current]" or ""
      local label = string.format("[wezterm %s] %s%s", item.kind, title, state)
      if #context > 0 then label = label .. " — " .. table.concat(context, " · ") end
      table.insert(rows, {
        label=M.sanitize(label, MAX_LABEL),
        provider="wezterm", kind=item.kind,
        group=item.current and 0 or (item.workspace == options.current_workspace and 1 or 3),
        endpoint_group="", activity=activity(item.activity), canonical=identity_native(item),
        qualifier=M.sanitize(item.workspace) .. " " .. M.sanitize(item.mux_window_id) .. " " .. M.sanitize(item.pane_id),
        route={provider="wezterm", kind=item.kind, identity={
          domain=tostring(item.domain or ""), workspace=tostring(item.workspace or ""),
          mux_window_id=tostring(item.mux_window_id or ""), tab_id=tostring(item.tab_id or ""),
          pane_id=tostring(item.pane_id or "")
        }, current=item.current == true}
      })
    end
  end
end

local function add_tmux(rows, inventory, native, options, diagnostics)
  for _, endpoint in ipairs(inventory.endpoints) do
    if endpoint._unknown_status then
      table.insert(diagnostics, "Skipped endpoint with unknown status")
    elseif endpoint.status == "available" or endpoint.status == "partial" then
      local match = choose_client_candidates(native, endpoint, options)
      local sessions, windows = {}, {}
      for _, session in ipairs(endpoint.sessions) do sessions[session.session_id] = session end
      for _, window in ipairs(endpoint.windows) do windows[window.session_id .. "\31" .. window.window_id] = window end
      local function route(kind, session_id, window_id, pane_id)
        local client = match and match.client or nil
        return {
          provider="tmux", kind=kind, host_domain=endpoint.host_domain, endpoint_id=endpoint.endpoint_id,
          generation=endpoint.generation.token, session_id=session_id,
          window_id=window_id, pane_id=pane_id, host_pane=match and match.pane or nil,
          client=client and {
            endpoint_id=client.endpoint_id, generation=client.generation,
            client_name=client.client_name, client_tty=client.client_tty,
            client_pid=client.client_pid, client_created=client.client_created,
            client_uid=client.client_uid
          } or nil
        }
      end
      local function emit(kind, record, primary, context, session_id, window_id, pane_id)
        local attached, current = false, false
        for _, client in ipairs(endpoint.clients) do
          if client_at(client, kind, session_id, window_id, pane_id) then attached = true end
          if match and client.client_name == match.client.client_name and client_at(client, kind, session_id, window_id, pane_id) then current = true end
        end
        local state = current and " [current]" or (attached and " [attached]" or "")
        local label = string.format("[tmux %s] %s%s", kind, M.sanitize(primary), state)
        local terms = {}
        for _, value in ipairs(context) do
          value = M.sanitize(value)
          if value ~= "" then table.insert(terms, value) end
        end
        table.insert(terms, M.sanitize(endpoint.alias))
        if #terms > 0 then label = label .. " — " .. table.concat(terms, " · ") end
        local canonical = table.concat({endpoint.host_domain, endpoint.endpoint_id, endpoint.generation.token, kind, session_id or "", window_id or "", pane_id or ""}, "\31")
        table.insert(rows, {
          label=M.sanitize(label, MAX_LABEL), provider="tmux", kind=kind,
          group=match and 2 or 4, endpoint_group=endpoint.alias,
          activity=activity(record.activity), canonical=canonical,
          qualifier=M.sanitize(endpoint.alias) .. " " .. M.sanitize(session_id) .. " " .. M.sanitize(window_id) .. " " .. M.sanitize(pane_id),
          route=route(kind, session_id, window_id, pane_id)
        })
      end
      for _, session in ipairs(endpoint.sessions) do
        emit("session", session, session.name, {basename(session.path), session.note}, session.session_id)
      end
      for _, window in ipairs(endpoint.windows) do
        local session = sessions[window.session_id]
        emit("window", window, window.name, {session and session.name, window.index, window.note}, window.session_id, window.window_id)
      end
      for _, pane in ipairs(endpoint.panes) do
        local session = sessions[pane.session_id]
        local window = windows[pane.session_id .. "\31" .. pane.window_id]
        local primary = pane.title or pane.command or pane.pane_id
        emit("pane", pane, primary, {session and session.name, window and window.name, pane.command, basename(pane.path), pane.note}, pane.session_id, pane.window_id, pane.pane_id)
      end
      if endpoint.status == "partial" then table.insert(diagnostics, "tmux endpoint " .. M.sanitize(endpoint.alias) .. " is partial") end
    else
      table.insert(diagnostics, "tmux endpoint " .. M.sanitize(endpoint.alias) .. " is unavailable")
    end
  end
end

local function disambiguate(rows)
  local groups = {}
  for _, row in ipairs(rows) do
    groups[row.label] = groups[row.label] or {}
    table.insert(groups[row.label], row)
  end
  for _, group in pairs(groups) do
    if #group > 1 then
      local seen = {}
      for _, row in ipairs(group) do
        local candidate = M.sanitize(row.qualifier, 96)
        if candidate == "" or seen[candidate] then candidate = row.canonical:sub(-12) end
        seen[candidate] = true
        local suffix = " [" .. candidate .. "]"
        row.label = truncate(row.label, MAX_LABEL - #suffix) .. suffix
      end
    end
  end
end

function M.build(native, inventory, options)
  native, options = native or {}, options or {}
  local rows, diagnostics = {}, {}
  add_native(rows, native, options)
  if inventory ~= nil then
    local valid, err = validate_inventory(inventory)
    if valid then
      add_tmux(rows, valid, native, options, diagnostics)
    else
      table.insert(diagnostics, "tmux inventory ignored: " .. M.sanitize(err, 160))
    end
  end
  disambiguate(rows)
  table.sort(rows, function(a, b)
    if a.group ~= b.group then return a.group < b.group end
    if a.endpoint_group ~= b.endpoint_group then return a.endpoint_group < b.endpoint_group end
    if a.activity ~= b.activity then return a.activity > b.activity end
    if a.label ~= b.label then return a.label < b.label end
    return a.canonical < b.canonical
  end)
  local max_rows = math.min(math.max(1, tonumber(options.max_rows) or 10000), 10000)
  local choices, routes = {}, {}
  for i = 1, math.min(#rows, max_rows) do
    local id = string.format("tmx-choice-%05d", i)
    choices[#choices + 1] = {id=id, label=rows[i].label}
    routes[id] = rows[i].route
  end
  if #rows > max_rows then table.insert(diagnostics, "destination rows truncated at " .. max_rows) end
  return {choices=choices, routes=routes, diagnostics=diagnostics}
end

return M
