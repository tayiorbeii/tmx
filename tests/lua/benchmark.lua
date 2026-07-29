package.path = './wezterm/?.lua;./wezterm/?/init.lua;' .. package.path
local model = require 'tmx_switcher.model'
local native = {}
for i=1,1000 do
  native[i] = {
    kind='pane', domain='local', workspace='bench-' .. (i % 5),
    mux_window_id=tostring(math.floor(i/10)), tab_id=tostring(math.floor(i/2)),
    pane_id=tostring(i), title='pane ' .. i, path='/tmp/project/' .. i,
    command='process-' .. (i % 20), tty='/dev/ttys' .. i, current=i == 1,
  }
end
local samples = {}
for iteration=1,20 do
  local started = os.clock()
  local result = model.build(native, nil, {current_workspace='bench-1',max_rows=10000})
  assert(#result.choices == 1000)
  samples[#samples+1] = (os.clock()-started)*1000
end
table.sort(samples)
local p95 = samples[math.ceil(#samples*0.95)]
print(string.format('lua_1000_targets p50=%.3fms p95=%.3fms max=%.3fms', samples[math.ceil(#samples*0.5)], p95, samples[#samples]))
assert(p95 <= 250, string.format('1000-target p95 %.3fms exceeds 250ms', p95))
