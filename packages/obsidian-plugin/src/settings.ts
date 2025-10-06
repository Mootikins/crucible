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

    // TODO: Add button to fetch available models from API
  }
}
