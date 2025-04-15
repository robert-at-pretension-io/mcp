import { SystemMessage } from './Message.js'; // Import specific types if needed
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
     * Compacts the conversation history when it gets too long
     * by summarizing older messages
     */
    async compactHistory(summarizePrompt, aiClient) {
        if (this.history.length < 10)
            return; // Don't compact if too few messages
        // Keep the most recent messages (e.g., last 5 exchanges/10 messages)
        const recentMessages = this.history.slice(-10);
        const olderMessages = this.history.slice(0, -10);
        if (olderMessages.length === 0)
            return;
        // Format older messages for summarization
        const historyString = olderMessages.map(msg => {
            const role = msg._getType();
            const content = typeof msg.content === 'string' ? msg.content : JSON.stringify(msg.content);
            return `${role.toUpperCase()}: ${content}`;
        }).join('\n\n');
        // Replace placeholder in the summarization prompt
        const prompt = summarizePrompt.replace('{history_string}', historyString);
        try {
            // Use the same AI client to generate a summary
            const summaryMessage = new SystemMessage(`[Previous conversation summary: ${prompt}]`);
            // Replace older messages with summary
            this.history = [summaryMessage, ...recentMessages];
        }
        catch (error) {
            console.error('Failed to compact conversation history:', error);
            // If summarization fails, just keep the recent messages
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