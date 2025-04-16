import type { ConversationMessage } from '../conversation/Message.js';

export interface IAiClient {
  /**
   * Generates a response from the AI based on the message history.
   * @param messages - The history of messages in the conversation.
   * @returns The AI's response content as a string.
   * @throws {Error} If the AI request fails.
   */
  generateResponse(messages: ConversationMessage[]): Promise<string>;

  /**
   * Gets the identifier (model name) of the underlying AI model being used.
   */
  getModelName(): string;
  
  /**
   * Gets the provider name (e.g., "openai", "anthropic").
   * This is optional for backwards compatibility.
   */
  getProvider?(): string;
}
