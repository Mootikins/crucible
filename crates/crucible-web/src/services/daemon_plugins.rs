//! `plugin.*` RPCs, forwarded to the daemon.
//!
//! Split from `daemon.rs` for the 1500-line file budget, along a real seam:
//! these are the calls that serve the plugin panel and its settings pane, and
//! none of them interpret what a plugin's data means — that is the whole point
//! of the publications and options channels.

use super::daemon::ReconnectingDaemon;

impl ReconnectingDaemon {
    pub async fn plugin_list_info(&self) -> anyhow::Result<Vec<serde_json::Value>> {
        self.call_with_reconnect("plugin.list", |daemon| Box::pin(daemon.plugin_list_info()))
            .await
    }

    pub async fn plugin_publications(&self) -> anyhow::Result<serde_json::Value> {
        self.call_with_reconnect("plugin.publications", |daemon| {
            Box::pin(daemon.plugin_publications())
        })
        .await
    }

    /// The settings trees plugins declared. `ui` is always "web" from here —
    /// it is what makes `webHidden` mean something.
    pub async fn plugin_options(&self) -> anyhow::Result<serde_json::Value> {
        self.call_with_reconnect("plugin.options", |daemon| {
            Box::pin(daemon.plugin_options("web"))
        })
        .await
    }

    pub async fn plugin_option_get(
        &self,
        plugin: &str,
        path: Vec<String>,
    ) -> anyhow::Result<serde_json::Value> {
        let (plugin, path) = (plugin.to_string(), path);
        self.call_with_reconnect("plugin.option_get", move |daemon| {
            let (plugin, path) = (plugin.clone(), path.clone());
            Box::pin(async move { daemon.plugin_option_get(&plugin, &path, "web").await })
        })
        .await
    }

    pub async fn plugin_option_set(
        &self,
        plugin: &str,
        path: Vec<String>,
        value: serde_json::Value,
    ) -> anyhow::Result<()> {
        let plugin = plugin.to_string();
        self.call_with_reconnect("plugin.option_set", move |daemon| {
            let (plugin, path, value) = (plugin.clone(), path.clone(), value.clone());
            Box::pin(async move { daemon.plugin_option_set(&plugin, &path, value, "web").await })
        })
        .await
    }

    pub async fn plugin_option_execute(
        &self,
        plugin: &str,
        path: Vec<String>,
    ) -> anyhow::Result<()> {
        let plugin = plugin.to_string();
        self.call_with_reconnect("plugin.option_execute", move |daemon| {
            let (plugin, path) = (plugin.clone(), path.clone());
            Box::pin(async move { daemon.plugin_option_execute(&plugin, &path, "web").await })
        })
        .await
    }

    pub async fn plugin_reload(&self, name: &str) -> anyhow::Result<serde_json::Value> {
        let name = name.to_string();
        self.call_with_reconnect("plugin.reload", move |daemon| {
            let name = name.clone();
            Box::pin(async move { daemon.plugin_reload(&name).await })
        })
        .await
    }

    pub async fn plugin_install(
        &self,
        url: &str,
        branch: Option<&str>,
        pin: Option<&str>,
    ) -> anyhow::Result<serde_json::Value> {
        let url = url.to_string();
        let branch = branch.map(str::to_string);
        let pin = pin.map(str::to_string);
        self.call_with_reconnect("plugin.install", move |daemon| {
            let url = url.clone();
            let branch = branch.clone();
            let pin = pin.clone();
            Box::pin(async move {
                daemon
                    .plugin_install(&url, branch.as_deref(), pin.as_deref())
                    .await
            })
        })
        .await
    }

    pub async fn plugin_remove(
        &self,
        name: &str,
        purge: bool,
    ) -> anyhow::Result<serde_json::Value> {
        let name = name.to_string();
        self.call_with_reconnect("plugin.remove", move |daemon| {
            let name = name.clone();
            Box::pin(async move { daemon.plugin_remove(&name, purge).await })
        })
        .await
    }
}
