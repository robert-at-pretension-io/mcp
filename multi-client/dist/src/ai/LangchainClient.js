export class LangchainClient {
    chatModel;
    modelIdentifier; // The specific model identifier being used
    constructor(chatModel, modelIdentifier) {
        this.chatModel = chatModel;
        this.modelIdentifier = modelIdentifier;
    }
    async generateResponse(messages) {
        try {
            // Ensure messages are in the format LangChain expects (they should be if using BaseMessage)
            const response = await this.chatModel.invoke(messages);
            if (typeof response.content === 'string') {
                return response.content;
            }
            else {
                // Handle potential non-string content (e.g., structured output)
                console.warn('AI response content is not a simple string:', response.content);
                // Attempt to stringify, or handle based on expected complex types later
                return JSON.stringify(response.content);
            }
        }
        catch (error) {
            console.error(`Langchain AI request failed for model ${this.modelIdentifier}:`, error);
            throw new Error(`AI request failed: ${error instanceof Error ? error.message : String(error)}`);
        }
    }
    getModelName() {
        return this.modelIdentifier;
    }
}
//# sourceMappingURL=LangchainClient.js.map