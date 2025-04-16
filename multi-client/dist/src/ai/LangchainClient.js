export class LangchainClient {
    // Use RunnableInterface which is the return type of bindTools
    runnable;
    modelIdentifier; // The specific model identifier being used
    providerName; // The provider name (e.g., "openai", "anthropic")
    constructor(runnable, modelIdentifier, providerName) {
        this.runnable = runnable;
        this.modelIdentifier = modelIdentifier;
        // Determine provider name - this inference might be less reliable now
        // Rely primarily on the providerName passed in
        if (providerName) {
            this.providerName = providerName;
        }
        else {
            // Basic fallback inference based on model name
            if (modelIdentifier.includes('gpt-')) {
                this.providerName = 'openai';
            }
            else if (modelIdentifier.includes('claude')) {
                this.providerName = 'anthropic';
            }
            else if (constructorName.includes('googlegenai') || constructorName.includes('gemini')) {
                this.providerName = 'google-genai';
            }
            else if (constructorName.includes('mistral')) {
                this.providerName = 'mistralai';
            }
            else if (constructorName.includes('fireworks')) {
                this.providerName = 'fireworks';
            }
            else {
                this.providerName = 'unknown';
            }
        }
    }
    async generateResponse(messages) {
        try {
            // Invoke the runnable directly
            const response = await this.runnable.invoke(messages);
            // Ensure content is a string
            if (typeof response.content === 'string') {
                return response.content;
            }
            else if (Array.isArray(response.content)) {
                // Handle array content (e.g., from tool use structure) - extract text parts
                return response.content
                    .filter(item => typeof item === 'object' && item?.type === 'text')
                    .map(item => item.text)
                    .join('\n');
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
    getProvider() {
        return this.providerName;
    }
}
//# sourceMappingURL=LangchainClient.js.map