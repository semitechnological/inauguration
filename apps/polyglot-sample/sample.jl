function answer()
    return 42
end

function main()
    value = answer()
    return nothing
end

mutable struct Counter
    value::Int

    function inc(self)
        self.value += 1
        return self.value
    end
end
