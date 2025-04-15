import type { ConversationMessage } from './Message.js';
import { SystemMessage } from './Message.js'; // Import specific types if needed

export class ConversationState {
  // Store messages in the order they occurred
  private history: ConversationMessage[] = [];
  private systemPromptMessage: SystemMessage | null = null;

  constructor(initialSystemPrompt?: string) {
    if (initialSystemPrompt) {
      this.systemPromptMessage = new SystemMessage(initialSystemPrompt);
    }
  }

  /**
   * Adds a message to the conversation history.
   */
  addMessage(message: ConversationMessage): void {
    this.history.push(message);
  }

  /**
   * Sets or updates the system prompt. This will be the first message sent to the AI.
   */
  setSystemPrompt(prompt: string): void {
    this.systemPromptMessage = new SystemMessage(prompt);
  }

  /**
   * Gets the full message history, including the system prompt if set.
   */
  getMessages(): ConversationMessage[] {
    // Return a copy to prevent external modification
    const messages = [...this.history];
    if (this.systemPromptMessage) {
      // Ensure system prompt is always first
      messages.unshift(this.systemPromptMessage);
    }
    return messages;
  }

  /**
   * Gets only the messages excluding the system prompt.
   */
  getHistoryWithoutSystemPrompt(): ConversationMessage[] {
    return [...this.history];
  }

  /**
   * Clears the conversation history (excluding the system prompt).
   */
  clearHistory(): void {
    this.history = [];
  }

  /**
   * Replaces the entire history with a new set of messages.
   * Does not affect the system prompt.
   */
  replaceHistory(messages: ConversationMessage[]): void {
    this.history = [...messages];
  }

  // TODO: Add methods for compaction later if needed
}
