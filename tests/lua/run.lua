package.path = './wezterm/?.lua;./wezterm/?/init.lua;./tests/lua/?.lua;' .. package.path

local files = {'json_spec', 'model_spec', 'adapter_spec'}
local passed = 0
for _, name in ipairs(files) do
  local suite = require(name)
  local tests = {}
  for test_name, fn in pairs(suite) do tests[#tests + 1] = {name=test_name, fn=fn} end
  table.sort(tests, function(a, b) return a.name < b.name end)
  for _, test in ipairs(tests) do
    local ok, err = pcall(test.fn)
    if not ok then
      io.stderr:write(string.format('FAIL %s.%s: %s\n', name, test.name, tostring(err)))
      os.exit(1)
    end
    passed = passed + 1
    io.write(string.format('ok %s.%s\n', name, test.name))
  end
end
io.write(string.format('%d Lua switcher tests passed\n', passed))
