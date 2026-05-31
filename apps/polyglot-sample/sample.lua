function answer()
  return 42
end

function main()
end

-- Lua "class" via table
function makeCounter(start)
  local counter = {
    value = start
  }
  function counter:inc()
    self.value = self.value + 1
    return self.value
  end
  return counter
end
