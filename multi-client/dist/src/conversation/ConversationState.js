import { SystemMessage, HumanMessage } from './Message.js'; // Import specific types if needed
export class ConversationState {
    // Store messages in the order they occurred
    history = [];
    systemPromptMessage = null;
    verificationState = null;
    currentTurn = 0; // Track the current conversation turn
    constructor(initialSystemPrompt) {
        if (initialSystemPrompt) {
            this.systemPromptMessage = new SystemMessage(initialSystemPrompt);
        }
    }
    /**
     * Sets or updates the system prompt. This will be the first message sent to the AI.
     */
    setSystemPrompt(prompt) {
        this.systemPromptMessage = new SystemMessage(prompt);
    }
    /**
     * Gets the full message history, including the system prompt if set.
     */
    getMessages() {
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
    getHistoryWithoutSystemPrompt() {
        return [...this.history];
    }
    /**
     * Clears the conversation history (excluding the system prompt).
     */
    clearHistory() {
        this.history = [];
        this.verificationState = null;
        this.currentTurn = 0;
    }
    /**
     * Replaces the entire history with a new set of messages.
     * Does not affect the system prompt.
     */
    replaceHistory(messages) {
        this.history = [...messages];
    }
    /**
     * Gets the current conversation turn number
     */
    getCurrentTurn() {
        return this.currentTurn;
    }
    /**
     * Increments the turn counter (called when a human message is added)
     */
    incrementTurn() {
        this.currentTurn++;
    }
    /**
     * Sets the verification criteria for the current conversation
     */
    setVerificationState(originalRequest, criteria) {
        this.verificationState = {
            originalRequest,
            criteria,
            turnIndex: this.currentTurn
        };
    }
    /**
     * Gets the current verification criteria if set
     */
    getVerificationState() {
        return this.verificationState;
    }
    /**
     * Gets a formatted string of the conversation sequence for verification
     */
    getRelevantSequenceForVerification() {
        if (!this.verificationState)
            return '';
        // Extract messages from the relevant turn onward
        const relevantMessages = this.history.filter((_, index) => {
            // Get the position in the actual history array (ignoring system prompt)
            return index >= this.verificationState.turnIndex;
        });
        // Format the messages for verification
        return relevantMessages.map(msg => {
            const role = msg._getType();
            let content = '';
            if (typeof msg.content === 'string') {
                content = msg.content;
            }
            else {
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
    async compactHistory(compactionPromptTemplate, aiClient) {
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
            const compactionMessages = [
                new SystemMessage("You are an expert conversation summarizer."), // System context for this task
                new HumanMessage(promptText)
            ];
            // Call the AI client to generate the summary
            const summaryContent = await aiClient.generateResponse(compactionMessages);
            // Prepend the summary to the *existing* system prompt content
            const originalSystemPromptContent = this.systemPromptMessage?.content || '';
            // Ensure a clear separation between summary and original prompt
            const combinedSystemPromptContent = `[Previous conversation summary:\n${summaryContent}\n]\n\n${originalSystemPromptContent}`;
            this.setSystemPrompt(combinedSystemPromptContent); // Update the system prompt message
            // Replace the history with only the recent messages
            this.history = recentMessages;
            console.log('[State] Successfully compacted conversation history.');
        }
        catch (error) {
            console.error('[State] Failed to compact conversation history:', error);
            // Decide on fallback: Keep original history? Or just recent messages?
            // Keeping just recent messages might lose context but prevents unbounded growth.
            console.warn('[State] Compaction failed. Keeping only recent messages.');
            this.history = recentMessages;
        }
    }
    /**
     * Adds a message to the conversation history and tracks turns.
     */
    addMessage(message) {
        this.history.push(message);
        // If it's a human message, increment the turn counter
        if (message._getType() === 'human') {
            this.incrementTurn();
        }
    }
}
//# sourceMappingURL=ConversationState.js.map