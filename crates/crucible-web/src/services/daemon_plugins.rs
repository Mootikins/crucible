//! `plugin.*` RPCs, forwarded to the daemon.
//!
//! Split from `daemon.rs` along a real seam:
//! these are the calls that serve the plugin panel and its settings pane, and
//! none of them interpret what a plugin's data means — that is the whole point
//! of the publications and options channels.

use super::daemon::ReconnectingDaemon;

impl ReconnectingDaemon {
    forward_rpc! {
        Safe PluginList =>
        plugin_list_info()
        -> Vec<serde_json::Value> = plugin_list_info();
    }

    forward_rpc! {
        /// Every command loaded plugins declared, with its declared parameters.
        ///
        /// The enumeration a caller needs before it can offer a primitive as a
        /// button: `commands_json` already emits `name`, `description`, `hint` and
        /// `parameters` from the same `ToolDefinition` a tool uses, and until now
        /// it reached the daemon's own clients and no browser.
        Safe PluginCommands =>
        plugin_commands()
        -> Vec<serde_json::Value> = plugin_commands();
    }

    forward_rpc! {
        /// Surfaces plugins declared, rows included.
        ///
        /// Passed through verbatim, exactly as publications are: nothing on this
        /// side knows what a plugin's rows mean. A row is `{id, text, detail, mark}`
        /// and the component draws it from that, so a plugin shipped tomorrow gets a
        /// panel with no change here.
        Safe SurfaceList =>
        surfaces()
        -> serde_json::Value = surface_list();
    }

    forward_rpc! {
        Safe PluginPublications =>
        plugin_publications(key: Option<String>)
        -> serde_json::Value = plugin_publications(key.as_deref());
    }

    forward_rpc! {
        /// The settings trees plugins declared. `ui` is always "web" from here —
        /// it is what makes `webHidden` mean something.
        Safe PluginOptions =>
        plugin_options()
        -> serde_json::Value = plugin_options("web");
    }

    forward_rpc! {
        Safe PluginOptionGet =>
        plugin_option_get(plugin: &str, path: Vec<String>)
        -> serde_json::Value = plugin_option_get(&plugin, &path, "web");
    }

    forward_rpc! {
        Once PluginOptionSet =>
        plugin_option_set(plugin: &str, path: Vec<String>, value: serde_json::Value)
        -> () = plugin_option_set(&plugin, &path, value, "web");
    }

    forward_rpc! {
        Once PluginOptionExecute =>
        plugin_option_execute(plugin: &str, path: Vec<String>)
        -> () = plugin_option_execute(&plugin, &path, "web");
    }

    forward_rpc! {
        /// Invoke a plugin command and hand back whatever its Lua `fn` returned.
        ///
        /// The channel by which a target provider is asked to enumerate itself —
        /// a branch list depends on which project is selected and on what happened
        /// in the repo since, so it cannot be published once and cached.
        Once PluginRunCommand =>
        plugin_run_command(name: &str, args: serde_json::Value)
        -> serde_json::Value = plugin_run_command(&name, args);
    }

    forward_rpc! {
        Once PluginReload =>
        plugin_reload(name: &str)
        -> serde_json::Value = plugin_reload(&name);
    }

    forward_rpc! {
        Once PluginInstall =>
        plugin_install(url: &str, branch: Option<&str> => branch.map(str::to_owned), pin: Option<&str> => pin.map(str::to_owned))
        -> serde_json::Value = plugin_install(&url, branch.as_deref(), pin.as_deref());
    }

    forward_rpc! {
        Once PluginRemove =>
        plugin_remove(name: &str, purge: bool)
        -> serde_json::Value = plugin_remove(&name, purge);
    }
}
