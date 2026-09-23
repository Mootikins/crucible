return {
  commands = {
    clear = {
      desc = "Clear the current session's model context",
      fn = function(args)
        return (cru :: any).builtin_clear(args)
      end,
    },
  },
}
