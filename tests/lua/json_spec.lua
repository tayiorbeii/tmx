local json = require 'tmx_switcher.json'
local suite = {}

local function equal(actual, expected)
  assert(actual == expected, string.format('expected %s, got %s', tostring(expected), tostring(actual)))
end

function suite.decodes_contract_values()
  local value = json.decode('{"schema":{"major":1},"ok":true,"items":["x",null,2]}')
  equal(value.schema.major, 1)
  equal(value.ok, true)
  equal(value.items[1], 'x')
  equal(value.items[2], json.null)
  equal(value.items[3], 2)
end

function suite.rejects_duplicate_keys()
  local ok, err = pcall(json.decode, '{"schema":1,"schema":2}')
  assert(not ok)
  assert(tostring(err):match('duplicate object key'))
end

function suite.rejects_depth_nodes_and_bytes()
  assert(not pcall(json.decode, '[[[[1]]]]', {max_depth=2}))
  assert(not pcall(json.decode, '[1,2,3]', {max_nodes=2}))
  assert(not pcall(json.decode, '"abcdef"', {max_bytes=3}))
end

function suite.rejects_invalid_raw_utf8_sequences()
  local cases = {
    string.char(0x80), string.char(0xe2, 0x82), string.char(0xc0, 0xaf),
    string.char(0xed, 0xa0, 0x80), string.char(0xf4, 0x90, 0x80, 0x80),
  }
  for _, bytes in ipairs(cases) do
    local ok, message = pcall(json.decode, '"' .. bytes .. '"')
    assert(not ok and tostring(message):match('invalid UTF%-8'))
  end
  assert(json.decode('"' .. string.char(0xf4, 0x8f, 0xbf, 0xbf) .. '"'))
end

function suite.rejects_invalid_numbers_and_trailing_data()
  assert(not pcall(json.decode, '{"x":01}'))
  assert(not pcall(json.decode, '{} garbage'))
  assert(not pcall(json.decode, '{"x":1e9999}'))
end

function suite.decodes_surrogate_pairs()
  local value = json.decode('{"x":"\\ud83d\\ude80"}')
  equal(value.x, '🚀')
end

return suite
