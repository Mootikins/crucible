--- Tests for {{name}} plugin

-- The plugin is required by its DIRECTORY NAME, never by `init`: the test
-- runner's package.path mirrors the daemon loader's, which exposes a plugin
-- as `<plugins-parent>/?/init.lua`.
describe("{{name}}", function()
    local plugin = require("{{name}}")


    it("should greet with default greeting", function()
        local result = plugin.tools.greet.fn({ name = "Alice" })
        expect.equal(result.message, "Hello, Alice!")
        expect.truthy(result.timestamp)
    end)
    
    it("should greet with custom greeting", function()
        local result = plugin.tools.greet.fn({ 
            name = "Bob", 
            greeting = "Hi" 
        })
        expect.equal(result.message, "Hi, Bob!")
    end)
    
    it("should use default name when not provided", function()
        local result = plugin.tools.greet.fn({})
        expect.equal(result.message, "Hello, World!")
    end)
end)
