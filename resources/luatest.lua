-- Exercises the parts of the Lua port most likely to be broken on bubble-os.
-- Not a Lua test suite: everything here is something this kernel or the newlib
-- porting layer had to be taught, in roughly the order it was built.
--
--     lua /res/luatest.lua
--
-- Every check runs under pcall, so one failure reports itself instead of
-- taking the rest of the run with it.

local failures = 0

local function check(name, fn)
	local ok, result = pcall(fn)
	if ok and result then
		print("ok    " .. name)
	else
		failures = failures + 1
		print("FAIL  " .. name .. (ok and "" or ("  " .. tostring(result))))
	end
end

-- pcall itself is setjmp/longjmp, so if this fails every other check is
-- meaningless and most of them would have died rather than reported
check("pcall catches an error", function()
	local ok, err = pcall(function() error("intentional") end)
	return ok == false and type(err) == "string"
end)

-- %.14g through newlib's float printf, which is compiled in or it is not
check("float formatting", function()
	return tostring(0.1 + 0.2) == "0.3"
end)

check("strtod", function()
	return tonumber("3.5") == 3.5 and tonumber("1e3") == 1000.0
end)

-- these live in libm, not the libc.a copy, so this is the -lm check
check("libm", function()
	return math.floor(2.7) == 2 and math.fmod(6.5, 3.0) == 0.5 and math.sqrt(16) == 4.0
end)

-- malloc and realloc, which reach the kernel through _sbrk
check("heap and gc", function()
	local parts = {}
	for i = 1, 2000 do
		parts[i] = tostring(i)
	end

	local joined = table.concat(parts, ",")
	collectgarbage()

	return #joined > 6000 and collectgarbage("count") > 0
end)

-- Lua's own call stack, which grows on the heap. Lua to Lua calls stay inside
-- luaV_execute rather than recursing in C, so this leans on the 512 KiB user
-- stack far less than it looks; the parser reading this file is the deeper
-- C recursion
check("recursion", function()
	local function depth(n)
		if n == 0 then return 0 end
		return 1 + depth(n - 1)
	end

	return depth(150) == 150
end)

-- environ, filled in by crt0.S from the entry frame the kernel writes
check("os.getenv", function()
	return os.getenv("PATH") ~= nil
end)

check("os.time and os.clock", function()
	return os.time() > 0 and type(os.clock()) == "number"
end)

check("file write and read back", function()
	local file = assert(io.open("/res/luatest.tmp", "w"))
	file:write("line one\n", "line two\n")
	file:close()

	local reopened = assert(io.open("/res/luatest.tmp", "r"))
	local first = reopened:read("l")
	reopened:close()

	return first == "line one"
end)

-- syscall 25, and the only thing that exercises it from C
check("os.rename", function()
	if not os.rename("/res/luatest.tmp", "/res/luatest2.tmp") then
		return false
	end

	local moved = io.open("/res/luatest2.tmp", "r")
	if not moved then return false end
	moved:close()

	return true
end)

check("os.remove", function()
	return os.remove("/res/luatest2.tmp") == true
end)

-- system() in syscalls.c, not newlib's stub. Programs are named by their full
-- filename: nothing appends .elf. Prints a listing of its own in the middle of
-- the output, which is the point rather than a mistake
check("os.execute", function()
	return os.execute("ls.elf") == true
end)

print("")
if failures == 0 then
	print("all checks passed")
else
	print(failures .. " check(s) failed")
end
