describe("kiln-expert", function()
  local plugin = require("init")

  describe("list_kilns", function()
    it("returns empty when no kilns configured", function()
      local result = plugin.tools.list_kilns.fn({})
      assert.equal(result.count, 0)
      assert.is_not_nil(result.kilns)
    end)
  end)

  describe("search_kiln", function()
    it("rejects missing kiln label", function()
      local result = plugin.tools.search_kiln.fn({ query = "test" })
      assert.truthy(result.error)
      assert.truthy(result.error:find("kiln label"))
    end)

    it("rejects missing query", function()
      local result = plugin.tools.search_kiln.fn({ kiln = "docs" })
      assert.truthy(result.error)
      assert.truthy(result.error:find("query"))
    end)

    it("rejects unknown kiln label", function()
      local result = plugin.tools.search_kiln.fn({ kiln = "nonexistent", query = "test" })
      assert.truthy(result.error)
      assert.truthy(result.error:find("unknown kiln"))
    end)

    it("lists available kilns in error for unknown label", function()
      -- setup with a known kiln first
      plugin.setup({ kilns = { docs = "/tmp/docs" } })
      local result = plugin.tools.search_kiln.fn({ kiln = "wrong", query = "test" })
      assert.truthy(result.error)
      assert.truthy(result.available)
      assert.truthy(result.available:find("docs"))
    end)
  end)

  describe("setup", function()
    it("accepts config table and updates kilns", function()
      plugin.setup({ kilns = { mykiln = "/tmp/mykiln" }, timeout = 45 })
      local result = plugin.tools.list_kilns.fn({})
      assert.truthy(result.count > 0)

      -- Verify the kiln is recognized (won't error on "unknown kiln")
      local search_result = plugin.tools.search_kiln.fn({ kiln = "mykiln", query = "test" })
      -- Will fail on session create (no daemon), but should NOT say "unknown kiln"
      if search_result.error then
        assert.falsy(search_result.error:find("unknown kiln"))
      end
    end)

    it("merges config without losing previous settings", function()
      plugin.setup({ kilns = { a = "/tmp/a" } })
      plugin.setup({ timeout = 99 })
      -- kilns should still have "a" (init merges, doesn't replace)
      local result = plugin.tools.list_kilns.fn({})
      assert.truthy(result.count > 0)
    end)
  end)
end)
