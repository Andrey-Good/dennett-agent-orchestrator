export type JsonPrimitive = string | number | boolean | null;

export interface JsonObject {
  [key: string]: JsonValue;
}

export interface JsonArray extends Array<JsonValue> {}

export type JsonValue = JsonPrimitive | JsonObject | JsonArray;

export type NonEmptyArray<T> = [T, ...T[]];

export type OpaqueObject = JsonObject;

export interface JsonObjectSchema extends JsonObject {
  type: "object";
}
