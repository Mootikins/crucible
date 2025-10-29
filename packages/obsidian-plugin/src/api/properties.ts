import { App, TFile } from "obsidian";
import { IncomingMessage, ServerResponse } from "http";

export class PropertiesHandler {
  constructor(private app: App) {}

  async updateProperties(filePath: string, req: IncomingMessage, res: ServerResponse) {
    try {
      const file = this.app.kiln.getAbstractFileByPath(filePath);

      if (!file || !(file instanceof TFile)) {
        this.sendJSON(res, 404, {
          error: "File not found",
          path: filePath,
        });
        return;
      }

      // Read request body
      const bodyStr = await this.readBody(req);
      const body = JSON.parse(bodyStr);
      const newProperties = body.properties || {};

      // Read current file content
      const content = await this.app.kiln.read(file);

      // Update frontmatter
      const updatedContent = this.updateFrontmatter(content, newProperties);

      // Write back to file
      await this.app.kiln.modify(file, updatedContent);

      this.sendJSON(res, 200, { success: true });
    } catch (error) {
      if (error instanceof SyntaxError) {
        this.sendJSON(res, 400, {
          error: "Invalid JSON",
          message: error.message,
        });
      } else {
        this.sendJSON(res, 500, {
          error: "Failed to update properties",
          message: error instanceof Error ? error.message : String(error),
        });
      }
    }
  }

  private updateFrontmatter(content: string, newProperties: Record<string, any>): string {
    const frontmatterMatch = content.match(/^---\n([\s\S]*?)\n---\n/);

    if (frontmatterMatch) {
      // Parse existing frontmatter
      const existingYaml = frontmatterMatch[1];
      let existingProperties: Record<string, any> = {};

      try {
        // Simple YAML parsing (for basic cases)
        existingProperties = this.parseSimpleYaml(existingYaml);
      } catch {
        // If parsing fails, replace frontmatter entirely
        existingProperties = {};
      }

      // Merge properties
      const mergedProperties = { ...existingProperties, ...newProperties };

      // Convert back to YAML
      const newYaml = this.toSimpleYaml(mergedProperties);

      // Replace frontmatter
      return content.replace(/^---\n[\s\S]*?\n---\n/, `---\n${newYaml}---\n`);
    } else {
      // No frontmatter exists, add it
      const newYaml = this.toSimpleYaml(newProperties);
      return `---\n${newYaml}---\n\n${content}`;
    }
  }

  private parseSimpleYaml(yaml: string): Record<string, any> {
    const result: Record<string, any> = {};
    const lines = yaml.split('\n');

    for (const line of lines) {
      const match = line.match(/^(\w+):\s*(.*)$/);
      if (match) {
        const [, key, value] = match;
        result[key] = this.parseYamlValue(value.trim());
      }
    }

    return result;
  }

  private parseYamlValue(value: string): any {
    // Handle arrays
    if (value.startsWith('[') && value.endsWith(']')) {
      const items = value.slice(1, -1).split(',').map(s => s.trim());
      return items.map(item => {
        // Remove quotes if present
        if ((item.startsWith('"') && item.endsWith('"')) ||
            (item.startsWith("'") && item.endsWith("'"))) {
          return item.slice(1, -1);
        }
        return item;
      });
    }

    // Handle numbers
    if (/^\d+$/.test(value)) {
      return parseInt(value, 10);
    }

    // Handle booleans
    if (value === 'true') return true;
    if (value === 'false') return false;

    // Handle quoted strings
    if ((value.startsWith('"') && value.endsWith('"')) ||
        (value.startsWith("'") && value.endsWith("'"))) {
      return value.slice(1, -1);
    }

    // Default to string
    return value;
  }

  private toSimpleYaml(obj: Record<string, any>): string {
    const lines: string[] = [];

    for (const [key, value] of Object.entries(obj)) {
      if (Array.isArray(value)) {
        const items = value.map(v => typeof v === 'string' ? `"${v}"` : v);
        lines.push(`${key}: [${items.join(', ')}]`);
      } else if (typeof value === 'object' && value !== null) {
        // Simple object handling
        lines.push(`${key}:`);
        for (const [subKey, subValue] of Object.entries(value)) {
          lines.push(`  ${subKey}: ${this.formatYamlValue(subValue)}`);
        }
      } else {
        lines.push(`${key}: ${this.formatYamlValue(value)}`);
      }
    }

    return lines.join('\n') + '\n';
  }

  private formatYamlValue(value: any): string {
    if (typeof value === 'string') {
      // Quote strings that might be interpreted as other types
      if (/^[\d.]+$/.test(value) || value === 'true' || value === 'false') {
        return `"${value}"`;
      }
      return value;
    }
    return String(value);
  }

  private async readBody(req: IncomingMessage): Promise<string> {
    return new Promise((resolve, reject) => {
      let body = "";
      req.on("data", (chunk) => (body += chunk));
      req.on("end", () => resolve(body));
      req.on("error", reject);
    });
  }

  private sendJSON(res: ServerResponse, status: number, data: any) {
    res.writeHead(status, { "Content-Type": "application/json" });
    res.end(JSON.stringify(data));
  }
}
