import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import { z } from "zod";
import { spawn } from "child_process";
import path from "node:path";
import fs from "node:fs";

const WORKING_DIR = path.resolve(__dirname, "..");
const TEST_DIR = "tests/e2e/playwright";

const server = new McpServer({
  name: "capsuled-playwright-mcp",
  version: "0.1.0",
});

// ツール1: Playwright テスト一覧
server.registerTool(
  "listTests",
  {
    description: "tests/e2e/playwright 以下の spec ファイル一覧を返します",
    inputSchema: {
      pattern: z
        .string()
        .optional()
        .describe("ファイル名フィルタ（例: 'gpu'）"),
    },
  },
  async ({ pattern }) => {
    const baseDir = path.join(WORKING_DIR, TEST_DIR);
    const files: string[] = [];

    try {
      const entries = fs.readdirSync(baseDir, { withFileTypes: true });
      for (const entry of entries) {
        if (!entry.isFile()) continue;
        if (!entry.name.endsWith(".spec.ts")) continue;
        if (pattern && !entry.name.includes(pattern)) continue;
        files.push(entry.name);
      }
    } catch (err: any) {
      return {
        content: [
          {
            type: "text",
            text: `Failed to read test directory "${baseDir}": ${err.message}`,
          },
        ],
        isError: true,
      };
    }

    if (files.length === 0) {
      return {
        content: [
          {
            type: "text",
            text: `No Playwright spec files found in ${TEST_DIR} (pattern=${
              pattern ?? "none"
            })`,
          },
        ],
      };
    }

    return {
      content: [
        {
          type: "text",
          text:
            `Found ${files.length} spec file(s) in ${TEST_DIR}:\n` +
            files.map((f) => `- ${f}`).join("\n"),
        },
      ],
    };
  }
);

// ツール2: Playwright テスト実行
server.registerTool(
  "runTests",
  {
    description: "Playwright の E2E テストを実行します",
    inputSchema: {
      testFilter: z
        .string()
        .optional()
        .describe("実行するテストのフィルタ（例: 'smoke.spec.ts'）"),
      project: z
        .string()
        .optional()
        .describe("Playwright の project 名（例: 'chromium'）"),
      extraArgs: z
        .array(z.string())
        .optional()
        .describe("追加の CLI 引数（例: ['--headed']）"),
    },
  },
  async ({ testFilter, project, extraArgs }) => {
    return new Promise((resolve) => {
      const cliArgs: string[] = ["playwright", "test"];

      if (testFilter?.trim()) {
        cliArgs.push(testFilter.trim());
      }

      if (project?.trim()) {
        cliArgs.push("--project", project.trim());
      }

      if (extraArgs && extraArgs.length > 0) {
        cliArgs.push(...extraArgs);
      }

      const child = spawn("npx", cliArgs, {
        cwd: WORKING_DIR,
        env: {
          ...process.env,
          CI: process.env.CI ?? "true",
        },
        shell: false,
      });

      let output = "";
      let errorOutput = "";

      child.stdout.on("data", (data) => {
        output += data.toString();
      });

      child.stderr.on("data", (data) => {
        errorOutput += data.toString();
      });

      child.on("close", (code) => {
        const summary: string[] = [];
        summary.push(`Command: npx ${cliArgs.join(" ")}`);
        summary.push(`CWD: ${WORKING_DIR}`);
        summary.push(`Exit code: ${code}`);

        if (output.trim()) {
          summary.push("\n=== STDOUT ===");
          summary.push(output.trim());
        }

        if (errorOutput.trim()) {
          summary.push("\n=== STDERR ===");
          summary.push(errorOutput.trim());
        }

        resolve({
          content: [
            {
              type: "text",
              text: summary.join("\n"),
            },
          ],
        });
      });
    });
  }
);

async function main() {
  const transport = new StdioServerTransport();
  await server.connect(transport);
}

main().catch((err) => {
  console.error("playwright-server fatal error:", err);
  process.exit(1);
});
