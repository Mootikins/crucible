--- Tests for {{name}} plugin

describe("{{name}}", function()
    local plugin = require("init")
    
    it("should greet with default greeting", function()
        local result = plugin.tools.greet.fn({ name = "Alice" })
        assert.equal(result.message, "Hello, Alice!")
        assert.truthy(result.timestamp)
    end)
    
    it("should greet with custom greeting", function()
        local result = plugin.tools.greet.fn({ 
            name = "Bob", 
            greeting = "Hi" 
        })
        assert.equal(result.message, "Hi, Bob!")
    end)
    
    it("should use default name when not provided", function()
        local result = plugin.tools.greet.fn({})
        assert.equal(result.message, "Hello, World!")
    end)
end)
