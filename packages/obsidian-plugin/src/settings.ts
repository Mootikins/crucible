import { App, PluginSettingTab, Setting } from "obsidian";
import MCPPlugin from "./main";

export class SettingsTab extends PluginSettingTab {
  plugin: MCPPlugin;

  constructor(app: App, plugin: MCPPlugin) {
    super(app, plugin);
    this.plugin = plugin;
  }

  display(): void {
    const { containerEl } = this;
    containerEl.empty();

    containerEl.createEl("h2", { text: "MCP Integration Settings" });

    // Server settings
    containerEl.createEl("h3", { text: "Server Configuration" });

    new Setting(containerEl)
      .setName("HTTP Server Port")
      .setDesc("Port for the local HTTP server (requires restart)")
      .addText((text) =>
        text
          .setPlaceholder("27123")
          .setValue(String(this.plugin.settings.port))
          .onChange(async (value) => {
            const port = parseInt(value);
            if (port > 0 && port < 65536) {
              this.plugin.settings.port = port;
              await this.plugin.saveSettings();
            }
          })
      );

    // Embedding settings
    containerEl.createEl("h3", { text: "Embedding Configuration" });

    new Setting(containerEl)
      .setName("Embedding Provider")
      .setDesc("Choose between OpenAI or Ollama for embeddings")
      .addDropdown((dropdown) =>
        dropdown
          .addOption("ollama", "Ollama (Local)")
          .addOption("openai", "OpenAI (Cloud)")
          .setValue(this.plugin.settings.embeddings.provider)
          .onChange(async (value: "openai" | "ollama") => {
            this.plugin.settings.embeddings.provider = value;
            await this.plugin.saveSettings();
            this.display(); // Refresh to show/hide API key field
          })
      );

    new Setting(containerEl)
      .setName("API URL")
      .setDesc(
        this.plugin.settings.embeddings.provider === "ollama"
          ? "Ollama server URL (e.g., http://localhost:11434)"
          : "OpenAI API base URL"
      )
      .addText((text) =>
        text
          .setPlaceholder(
            this.plugin.settings.embeddings.provider === "ollama"
              ? "http://localhost:11434"
              : "https://api.openai.com/v1"
          )
          .setValue(this.plugin.settings.embeddings.apiUrl)
          .onChange(async (value) => {
            this.plugin.settings.embeddings.apiUrl = value;
            await this.plugin.saveSettings();
          })
      );

    if (this.plugin.settings.embeddings.provider === "openai") {
      new Setting(containerEl)
        .setName("API Key")
        .setDesc("OpenAI API key")
        .addText((text) =>
          text
            .setPlaceholder("sk-...")
            .setValue(this.plugin.settings.embeddings.apiKey || "")
            .onChange(async (value) => {
              this.plugin.settings.embeddings.apiKey = value;
              await this.plugin.saveSettings();
            })
        );
    }

    new Setting(containerEl)
      .setName("Model")
      .setDesc("Embedding model to use")
      .addText((text) =>
        text
          .setPlaceholder(
            this.plugin.settings.embeddings.provider === "ollama"
              ? "nomic-embed-text"
              : "text-embedding-3-small"
          )
          .setValue(this.plugin.settings.embeddings.model)
          .onChange(async (value) => {
            this.plugin.settings.embeddings.model = value;
            await this.plugin.saveSettings();
          })
      );

    // MCP Client settings
    containerEl.createEl("h3", { text: "MCP Client Configuration" });

    new Setting(containerEl)
      .setName("Enable MCP Client")
      .setDesc("Enable connection to Rust MCP server via stdio")
      .addToggle((toggle) =>
        toggle
          .setValue(this.plugin.settings.mcp.enabled)
          .onChange(async (value) => {
            this.plugin.settings.mcp.enabled = value;
            await this.plugin.saveSettings();

            // Restart MCP client if needed
            if (value && this.plugin.settings.mcp.serverPath) {
              await this.plugin.startMcpClient();
            } else if (!value && this.plugin.mcpClient) {
              await this.plugin.stopMcpClient();
            }

            this.display(); // Refresh to show/hide MCP settings
          })
      );

    // MCP functionality has been removed - server settings disabled
      new Setting(containerEl)
        .setName("MCP Server Status")
        .setDesc("MCP functionality has been deprecated and removed")
        .addText((text) =>
          text
            .setValue("Disabled - MCP integration removed")
            .setDisabled(true)
        );

      new Setting(containerEl)
        .setName("Server Arguments")
        .setDesc("Command-line arguments for the MCP server (comma-separated)")
        .addText((text) =>
          text
            .setPlaceholder("--db-path, /path/to/vault.db")
            .setValue(this.plugin.settings.mcp.serverArgs.join(", "))
            .onChange(async (value) => {
              this.plugin.settings.mcp.serverArgs = value
                .split(",")
                .map((arg) => arg.trim())
                .filter((arg) => arg.length > 0);
              await this.plugin.saveSettings();
            })
        );

      new Setting(containerEl)
        .setName("Debug Mode")
        .setDesc("Enable debug logging for MCP communication")
        .addToggle((toggle) =>
          toggle
            .setValue(this.plugin.settings.mcp.debug)
            .onChange(async (value) => {
              this.plugin.settings.mcp.debug = value;
              await this.plugin.saveSettings();
            })
        );

      new Setting(containerEl)
        .setName("Restart MCP Client")
        .setDesc("Restart the MCP client with current settings")
        .addButton((button) =>
          button
            .setButtonText("Restart")
            .onClick(async () => {
              await this.plugin.restartMcpClient();
            })
        );

      // Show connection status
      const statusSetting = new Setting(containerEl)
        .setName("Connection Status")
        .setDesc(
          this.plugin.mcpClient?.isReady()
            ? `Connected to ${this.plugin.mcpClient.getServerInfo()?.server_info.name || "server"}`
            : "Not connected"
        );

      if (this.plugin.mcpClient?.isReady()) {
        statusSetting.descEl.style.color = "green";
      } else {
        statusSetting.descEl.style.color = "red";
      }
    }
  }
}
