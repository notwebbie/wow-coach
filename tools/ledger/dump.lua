-- Dump a SavedVariables global as JSON, for tools/ledger/ledger.py.
-- Usage: lua dump.lua <SavedVariables file> <GlobalName>
local path, name = arg[1], arg[2]
dofile(path)
local root = _G[name]
local function esc(s)
  return (s:gsub('[%c"\\]', function(c)
    if c == '"' then return '\\"' elseif c == '\\' then return '\\\\' end
    return string.format('\\u%04x', c:byte())
  end))
end
local function isarray(t)
  local n = 0
  for k in pairs(t) do
    if type(k) ~= 'number' or k < 1 or k % 1 ~= 0 then return false end
    n = n + 1
  end
  for i = 1, n do if t[i] == nil then return false end end
  return true, n
end
local out = {}
local function emit(v)
  local t = type(v)
  if t == 'table' then
    local arr, n = isarray(v)
    if arr and n > 0 then
      out[#out + 1] = '['
      for i = 1, n do if i > 1 then out[#out + 1] = ',' end emit(v[i]) end
      out[#out + 1] = ']'
    else
      out[#out + 1] = '{'
      local first = true
      for k, val in pairs(v) do
        if not first then out[#out + 1] = ',' end
        first = false
        out[#out + 1] = '"' .. esc(tostring(k)) .. '":'
        emit(val)
      end
      out[#out + 1] = '}'
    end
  elseif t == 'string' then out[#out + 1] = '"' .. esc(v) .. '"'
  elseif t == 'number' then
    if v ~= v or v == math.huge or v == -math.huge then out[#out + 1] = 'null'
    elseif v % 1 == 0 then out[#out + 1] = string.format('%d', v)
    else out[#out + 1] = string.format('%.6g', v) end
  elseif t == 'boolean' then out[#out + 1] = tostring(v)
  else out[#out + 1] = 'null' end
end
emit(root)
io.write(table.concat(out))
