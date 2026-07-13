import * as readline from "node:readline";
const rl = readline.createInterface({ input: process.stdin });
rl.on("line", (line) => {
  try {
    const msg = JSON.parse(line) as { id?: string; method?: string; params?: unknown };
    const result = msg.method === "health" ? { status: "healthy" } : { echo: msg.params ?? null };
    process.stdout.write(JSON.stringify({ id: msg.id ?? null, result }) + "\n");
  } catch (error) {
    process.stdout.write(JSON.stringify({ id: null, error: { code: "malformed_request", message: String(error) } }) + "\n");
  }
});
