import type { ConversationMessage } from './Message.js';
export interface VerificationState {
    originalRequest: string;
    criteria: string;
    turnIndex: number;
}
export declare class ConversationState {
    private history;
    private systemPromptMessage;
    private verificationState;
    private currentTurn;
    constructor(initialSystemPrompt?: string);
    /**
     * Sets or updates the system prompt. This will be the first message sent to the AI.
     */
    setSystemPrompt(prompt: string): void;
    /**
     * Gets the full message history, including the system prompt if set.
     */
    getMessages(): ConversationMessage[];
    /**
     * Gets only the messages excluding the system prompt.
     */
    getHistoryWithoutSystemPrompt(): ConversationMessage[];
    /**
     * Clears the conversation history (excluding the system prompt).
     */
    clearHistory(): void;
    /**
     * Replaces the entire history with a new set of messages.
     * Does not affect the system prompt.
     */
    replaceHistory(messages: ConversationMessage[]): void;
    /**
     * Gets the current conversation turn number
     */
    getCurrentTurn(): number;
    /**
     * Increments the turn counter (called when a human message is added)
     */
    incrementTurn(): void;
    /**
     * Sets the verification criteria for the current conversation
     */
    setVerificationState(originalRequest: string, criteria: string): void;
    /**
     * Gets the current verification criteria if set
     */
    getVerificationState(): VerificationState | null;
    /**
     * Gets a formatted string of the conversation sequence for verification
     */
    getRelevantSequenceForVerification(): string;
    /**
     * Compacts the conversation history by summarizing older messages.
     * @param compactionPromptTemplate The template for the summarization prompt (expecting {history_string}).
     * @param aiClient The AI client instance to use for summarization.
     */
    compactHistory(compactionPromptTemplate: string, aiClient: any): Promise<void>;
    /**
     * Adds a message to the conversation history and tracks turns.
     */
    addMessage(message: ConversationMessage): void;
}
