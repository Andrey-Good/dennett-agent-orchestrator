import { invoke } from "@tauri-apps/api/core";

export async function projectChat(text: string): Promise<string> {
  return invoke<string>("project_chat", { text });
}
