-- Strict, bounded JSON decoder for the machine contract.  It rejects duplicate
-- object keys before Lua tables can erase that evidence.
local M = { null = {} }
local ARRAY_MT = { __tmx_json_kind = "array" }
local OBJECT_MT = { __tmx_json_kind = "object" }

local function fail(pos, message)
  error(string.format("JSON error at byte %d: %s", pos, message), 0)
end

local function validate_utf8(text)
  local i = 1
  local function cont(at)
    local b = text:byte(at)
    return b and b >= 0x80 and b <= 0xbf
  end
  while i <= #text do
    local b, b2 = text:byte(i), text:byte(i + 1)
    if b <= 0x7f then i = i + 1
    elseif b >= 0xc2 and b <= 0xdf and cont(i + 1) then i = i + 2
    elseif b == 0xe0 and b2 and b2 >= 0xa0 and b2 <= 0xbf and cont(i + 2) then i = i + 3
    elseif ((b >= 0xe1 and b <= 0xec) or (b >= 0xee and b <= 0xef))
        and cont(i + 1) and cont(i + 2) then i = i + 3
    elseif b == 0xed and b2 and b2 >= 0x80 and b2 <= 0x9f and cont(i + 2) then i = i + 3
    elseif b == 0xf0 and b2 and b2 >= 0x90 and b2 <= 0xbf
        and cont(i + 2) and cont(i + 3) then i = i + 4
    elseif b >= 0xf1 and b <= 0xf3 and cont(i + 1) and cont(i + 2) and cont(i + 3) then i = i + 4
    elseif b == 0xf4 and b2 and b2 >= 0x80 and b2 <= 0x8f
        and cont(i + 2) and cont(i + 3) then i = i + 4
    else fail(i, "invalid UTF-8") end
  end
end

function M.decode(text, options)
  options = options or {}
  local max_bytes = options.max_bytes or (8 * 1024 * 1024)
  local max_depth = options.max_depth or 16
  local max_nodes = options.max_nodes or 50000
  if type(text) ~= "string" then error("JSON input must be a string", 0) end
  if #text > max_bytes then error("JSON input exceeds byte limit", 0) end
  validate_utf8(text)

  local pos, nodes = 1, 0
  local function bump()
    nodes = nodes + 1
    if nodes > max_nodes then fail(pos, "node limit exceeded") end
  end
  local function skip_space()
    while true do
      local byte = text:byte(pos)
      if byte == 32 or byte == 9 or byte == 10 or byte == 13 then
        pos = pos + 1
      else
        return
      end
    end
  end
  local function utf8(codepoint)
    if codepoint <= 0x7f then
      return string.char(codepoint)
    elseif codepoint <= 0x7ff then
      return string.char(0xc0 + math.floor(codepoint / 0x40), 0x80 + codepoint % 0x40)
    elseif codepoint <= 0xffff then
      return string.char(
        0xe0 + math.floor(codepoint / 0x1000),
        0x80 + math.floor(codepoint / 0x40) % 0x40,
        0x80 + codepoint % 0x40
      )
    end
    return string.char(
      0xf0 + math.floor(codepoint / 0x40000),
      0x80 + math.floor(codepoint / 0x1000) % 0x40,
      0x80 + math.floor(codepoint / 0x40) % 0x40,
      0x80 + codepoint % 0x40
    )
  end
  local function hex4(at)
    local value = tonumber(text:sub(at, at + 3), 16)
    if not value or #text:sub(at, at + 3) ~= 4 then fail(at, "invalid unicode escape") end
    return value
  end
  local function parse_string()
    if text:sub(pos, pos) ~= '"' then fail(pos, "expected string") end
    pos = pos + 1
    local pieces, start = {}, pos
    while pos <= #text do
      local byte = text:byte(pos)
      if byte == 34 then
        pieces[#pieces + 1] = text:sub(start, pos - 1)
        pos = pos + 1
        return table.concat(pieces)
      elseif byte == 92 then
        pieces[#pieces + 1] = text:sub(start, pos - 1)
        local escaped = text:sub(pos + 1, pos + 1)
        local simple = { ['"']='"', ['\\']='\\', ['/']='/', b='\b', f='\f', n='\n', r='\r', t='\t' }
        if simple[escaped] then
          pieces[#pieces + 1] = simple[escaped]
          pos = pos + 2
        elseif escaped == 'u' then
          local first = hex4(pos + 2)
          pos = pos + 6
          if first >= 0xd800 and first <= 0xdbff then
            if text:sub(pos, pos + 1) ~= '\\u' then fail(pos, "missing low surrogate") end
            local second = hex4(pos + 2)
            if second < 0xdc00 or second > 0xdfff then fail(pos + 2, "invalid low surrogate") end
            first = 0x10000 + (first - 0xd800) * 0x400 + (second - 0xdc00)
            pos = pos + 6
          elseif first >= 0xdc00 and first <= 0xdfff then
            fail(pos, "unexpected low surrogate")
          end
          pieces[#pieces + 1] = utf8(first)
        else
          fail(pos, "invalid string escape")
        end
        start = pos
      elseif byte < 32 then
        fail(pos, "unescaped control character")
      else
        pos = pos + 1
      end
    end
    fail(pos, "unterminated string")
  end

  local parse_value
  local function parse_array(depth)
    if depth > max_depth then fail(pos, "depth limit exceeded") end
    pos = pos + 1
    bump()
    local out = setmetatable({}, ARRAY_MT)
    skip_space()
    if text:sub(pos, pos) == ']' then pos = pos + 1; return out end
    while true do
      out[#out + 1] = parse_value(depth + 1)
      skip_space()
      local token = text:sub(pos, pos)
      if token == ']' then pos = pos + 1; return out end
      if token ~= ',' then fail(pos, "expected comma or closing bracket") end
      pos = pos + 1
      skip_space()
    end
  end
  local function parse_object(depth)
    if depth > max_depth then fail(pos, "depth limit exceeded") end
    pos = pos + 1
    bump()
    local out, seen = setmetatable({}, OBJECT_MT), {}
    skip_space()
    if text:sub(pos, pos) == '}' then pos = pos + 1; return out end
    while true do
      local key = parse_string()
      if seen[key] then fail(pos, "duplicate object key " .. key) end
      seen[key] = true
      skip_space()
      if text:sub(pos, pos) ~= ':' then fail(pos, "expected colon") end
      pos = pos + 1
      skip_space()
      out[key] = parse_value(depth + 1)
      skip_space()
      local token = text:sub(pos, pos)
      if token == '}' then pos = pos + 1; return out end
      if token ~= ',' then fail(pos, "expected comma or closing brace") end
      pos = pos + 1
      skip_space()
      if text:sub(pos, pos) ~= '"' then fail(pos, "expected object key") end
    end
  end
  local function parse_number()
    local start = pos
    if text:sub(pos, pos) == '-' then pos = pos + 1 end
    if text:sub(pos, pos) == '0' then
      pos = pos + 1
      if text:sub(pos, pos):match('%d') then fail(pos, "leading zero") end
    else
      local digits = text:match('^%d+', pos)
      if not digits then fail(pos, "invalid number") end
      pos = pos + #digits
    end
    if text:sub(pos, pos) == '.' then
      local fraction = text:match('^%.%d+', pos)
      if not fraction then fail(pos, "invalid fraction") end
      pos = pos + #fraction
    end
    local marker = text:sub(pos, pos)
    if marker == 'e' or marker == 'E' then
      local exponent = text:match('^[eE][+-]?%d+', pos)
      if not exponent then fail(pos, "invalid exponent") end
      pos = pos + #exponent
    end
    local value = tonumber(text:sub(start, pos - 1))
    if not value or value ~= value or value == math.huge or value == -math.huge then
      fail(start, "number is not finite")
    end
    bump()
    return value
  end
  function parse_value(depth)
    skip_space()
    local token = text:sub(pos, pos)
    if token == '"' then bump(); return parse_string() end
    if token == '{' then return parse_object(depth) end
    if token == '[' then return parse_array(depth) end
    if text:sub(pos, pos + 3) == 'true' then pos = pos + 4; bump(); return true end
    if text:sub(pos, pos + 4) == 'false' then pos = pos + 5; bump(); return false end
    if text:sub(pos, pos + 3) == 'null' then pos = pos + 4; bump(); return M.null end
    if token == '-' or token:match('%d') then return parse_number() end
    fail(pos, "unexpected token")
  end

  local value = parse_value(1)
  skip_space()
  if pos <= #text then fail(pos, "trailing content") end
  return value
end

return M
