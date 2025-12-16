import { test, expect } from "@playwright/test";

test.describe("MLX Integration", () => {
  test("should respond to health check", async ({ request }) => {
    const response = await request.get("http://127.0.0.1:8081/health");
    expect(response.ok()).toBeTruthy();
    const json = await response.json();
    expect(json.status).toBe("ok");
  });

  test("should generate text completion", async ({ request }) => {
    const response = await request.post(
      "http://127.0.0.1:8081/v1/chat/completions",
      {
        data: {
          model: "mlx-community/Qwen3-Next-80B-A3B-4bit",
          messages: [{ role: "user", content: "Hello" }],
          max_tokens: 10,
        },
      }
    );
    expect(response.ok()).toBeTruthy();
    const json = await response.json();
    expect(json.choices[0].message.content).toBeTruthy();
  });

  test("should support streaming", async ({ request }) => {
    const response = await request.post(
      "http://127.0.0.1:8081/v1/chat/completions",
      {
        data: {
          model: "mlx-community/Qwen3-Next-80B-A3B-4bit",
          messages: [{ role: "user", content: "Hello" }],
          stream: true,
          max_tokens: 10,
        },
      }
    );
    expect(response.ok()).toBeTruthy();
    expect(response.headers()["content-type"]).toContain("text/event-stream");
  });
});
