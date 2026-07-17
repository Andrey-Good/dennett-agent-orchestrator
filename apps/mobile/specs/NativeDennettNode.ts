export interface DennettNodeSpec {
  status(): Promise<string>;
  enqueueCommand(commandId: string, payloadJson: string): Promise<string>;
  emergencyStop(): Promise<void>;
}
