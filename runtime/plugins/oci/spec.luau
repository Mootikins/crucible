-- The fragment: what discovery reads about `oci` before any plugin code runs.
-- The intercept grant lives here and only here. The same flag in the
-- `init.luau` return table grants nothing, because discovery does not read
-- that file.
return {
  name = "oci",
  version = "0.2.0",
  description = "Run agent tools inside OCI containers via generic hook interception",
  author = "Crucible Team",
  license = "MIT",
  intercepts_tools = true,
}
