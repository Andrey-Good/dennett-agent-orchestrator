import type { JsonObject, JsonValue, NonEmptyArray } from "./json";

export const ORCHESTRATOR_USER_CHAT_SERVER_NAME = "orchestrator.user_chat" as const;

export type UserChatServerName = typeof ORCHESTRATOR_USER_CHAT_SERVER_NAME;

export interface UserChatTextRequestPayload {
  prompt_id?: string;
  kind: "text";
  text: string;
  require_response: boolean;
  options?: never;
}

export interface UserChatOptionsRequestOption {
  id: string;
  label: string;
  value: JsonValue;
}

export interface UserChatOptionsRequestPayload {
  prompt_id?: string;
  kind: "options";
  text: string;
  require_response: boolean;
  options: NonEmptyArray<UserChatOptionsRequestOption>;
}

export type UserChatRequestPayload =
  | UserChatTextRequestPayload
  | UserChatOptionsRequestPayload;

export interface UserChatTextResponsePayload {
  prompt_id?: string;
  kind: "text";
  text: string;
  value?: JsonValue;
  option_id?: never;
}

export interface UserChatOptionResponsePayload {
  prompt_id?: string;
  kind: "option";
  option_id: string;
  value: JsonValue;
  text?: never;
}

export type UserChatResponsePayload =
  | UserChatTextResponsePayload
  | UserChatOptionResponsePayload;

export interface UserChatRequestEvent {
  kind: "user_chat_request";
  payload: UserChatRequestPayload;
}

export interface UserChatResponseEnvelope {
  payload: UserChatResponsePayload;
  metadata?: JsonObject;
}

