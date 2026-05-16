/**
 * SoulSystem TypeScript SDK.
 *
 * In production, this would wrap a native library via napi-rs.
 */

export interface SoulMessage {
  type: string;
  [key: string]: any;
}

export class Bus {
  private messages: SoulMessage[] = [];

  publish(message: SoulMessage): void {
    this.messages.push(message);
  }

  receive(): SoulMessage | undefined {
    return this.messages.shift();
  }
}
