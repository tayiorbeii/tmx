local wezterm = require 'wezterm'
local config = wezterm.config_builder()
local here = wezterm.config_dir .. '/'
package.path = here .. '?.lua;' .. here .. '?/init.lua;' .. package.path
package.loaded.tmx_switcher = nil
local tmx_switcher = require 'tmx_switcher'

tmx_switcher.apply_to_config(config, wezterm, {
  -- Replace this with the absolute output of `command -v tmx`.
  tmx_bin = 'tmx',
  -- Derived from tmx_bin when both binaries are installed together.
  supervisor_bin = 'tmx-supervisor',
  enabled = true,
  allowed_local_domains = { 'local' },
  native_only_key = 'phys:0',
})

return config
