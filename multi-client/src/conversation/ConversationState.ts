import type { ConversationMessage } from './Message.js';
import { SystemMessage, HumanMessage } from './Message.js'; // Import specific types if needed

export interface VerificationState {
  originalRequest: string; // The original user request that generated criteria
  criteria: string; // The generated verification criteria
  turnIndex: number; // The conversation turn index when criteria were generated
}

export class ConversationState {
  // Store messages in the order they occurred
  private history: ConversationMessage[] = [];
  private systemPromptMessage: SystemMessage | null = null;
  private verificationState: VerificationState | null = null;
  private currentTurn: number = 0; // Track the current conversation turn

  constructor(initialSystemPrompt?: string) {
    if (initialSystemPrompt) {
      this.systemPromptMessage = new SystemMessage(initialSystemPrompt);
    }
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
    this.verificationState = null;
    this.currentTurn = 0;
  }

  /**
   * Replaces the entire history with a new set of messages.
   * Does not affect the system prompt.
   */
  replaceHistory(messages: ConversationMessage[]): void {
    this.history = [...messages];
  }

  /**
   * Gets the current conversation turn number
   */
  getCurrentTurn(): number {
    return this.currentTurn;
  }

  /**
   * Increments the turn counter (called when a human message is added)
   */
  incrementTurn(): void {
    this.currentTurn++;
  }

  /**
   * Sets the verification criteria for the current conversation
   */
  setVerificationState(originalRequest: string, criteria: string): void {
    this.verificationState = {
      originalRequest,
      criteria,
      turnIndex: this.currentTurn
    };
  }

  /**
   * Gets the current verification criteria if set
   */
  getVerificationState(): VerificationState | null {
    return this.verificationState;
  }

  /**
   * Gets a formatted string of the conversation sequence for verification
   */
  getRelevantSequenceForVerification(): string {
    if (!this.verificationState) return '';

    // Extract messages from the relevant turn onward
    const relevantMessages = this.history.filter((_, index) => {
      // Get the position in the actual history array (ignoring system prompt)
      return index >= this.verificationState!.turnIndex;
    });

    // Format the messages for verification
    return relevantMessages.map(msg => {
      const role = msg._getType();
      let content = '';

      if (typeof msg.content === 'string') {
        content = msg.content;
      } else {
        content = JSON.stringify(msg.content);
      }

      switch (role) {
        case 'human':
          return `User: ${content}`;
        case 'ai':
          return `Assistant: ${content}`;
        case 'tool':
          // @ts-ignore - accessing LangChain's internal properties
          const toolName = msg.name || 'Tool';
          return `Tool (${toolName}) Result: ${content}`;
        default:
          return `${role.charAt(0).toUpperCase() + role.slice(1)}: ${content}`;
      }
    }).join('\n\n');
  }

  /**
   * Compacts the conversation history by summarizing older messages.
   * @param compactionPromptTemplate The template for the summarization prompt (expecting {history_string}).
   * @param aiClient The AI client instance to use for summarization.
   */
  async compactHistory(compactionPromptTemplate: string, aiClient: any): Promise<void> {
    // Only compact if there's a reasonable number of messages beyond the keep threshold
    const messagesToKeep = 10; // Keep the last 5 exchanges (adjust as needed)
    const minMessagesToCompact = 4; // Require at least 2 exchanges to summarize

    if (this.history.length < messagesToKeep + minMessagesToCompact) {
        console.log(`[State] Skipping compaction: History length (${this.history.length}) is below threshold.`);
        return;
    }

    // Keep the most recent messages
    const recentMessages = this.history.slice(-messagesToKeep);
    const olderMessages = this.history.slice(0, -messagesToKeep);

    console.log(`[State] Compacting history: Keeping ${recentMessages.length}, summarizing ${olderMessages.length} messages.`);

    // Format older messages for the summarization prompt
    const historyString = olderMessages.map(msg => {
      const role = msg._getType();
      const content = typeof msg.content === 'string' ? msg.content : JSON.stringify(msg.content);
      return `${role.toUpperCase()}: ${content}`;
    }).join('\n\n');

    // Fill the compaction prompt template
    const promptText = compactionPromptTemplate.replace('{history_string}', historyString);

    try {
      // Create message list for the summarization call
      const compactionMessages: ConversationMessage[] = [
        new SystemMessage("You are an expert conversation summarizer."), // System context for this task
        new HumanMessage(promptText)
      ];

      // Call the AI client to generate the summary
      const summaryContent = await aiClient.generateResponse(compactionMessages);

      // --- Only modify state *after* successful summarization ---
      // Prepend the summary to the *existing* system prompt content
      const originalSystemPromptContent = this.systemPromptMessage?.content || '';
      // Ensure a clear separation between summary and original prompt
      const combinedSystemPromptContent = `[Previous conversation summary:\n${summaryContent}\n]\n\n${originalSystemPromptContent}`;
      this.setSystemPrompt(combinedSystemPromptContent); // Update the system prompt message

      // Replace the history with only the recent messages
      this.history = recentMessages;
      // --- End modification block ---

      console.log('[State] Successfully compacted conversation history.');
    } catch (error) {
      console.error('[State] Failed to compact conversation history:', error);
      // Do NOT modify history if compaction fails, log the error and continue with full history.
      console.warn('[State] Compaction failed. History remains unchanged.');
    }
  }

  /**
   * Adds a message to the conversation history and tracks turns.
   */
  addMessage(message: ConversationMessage): void {
    this.history.push(message);

    // If it's a human message, increment the turn counter
    if (message._getType() === 'human') {
      this.incrementTurn();
    }
  }

  /**
   * Removes the last message from history ONLY if it's an AI message marked as pending a tool call.
   * Used to clean up state when a tool loop is detected and broken.
   */
  removeLastMessageIfPendingAiToolCall(): void {
      const lastMessage = this.history[this.history.length - 1];
      if (
          lastMessage &&
          lastMessage._getType() === 'ai' &&
          (lastMessage as any).pendingToolCalls === true // Check our custom property
      ) {
          this.history.pop();
          console.log('[State] Removed last pending AI tool call message due to loop break.');
      }
  }
}
