import { ChatOpenAI } from '@langchain/openai';
import { ChatAnthropic } from '@langchain/anthropic';
import { ChatGoogleGenerativeAI } from '@langchain/google-genai';
import { ChatMistralAI } from '@langchain/mistralai';
import { ChatFireworks } from '@langchain/community/chat_models/fireworks';
import { LangchainClient } from './LangchainClient.js';
export class AiClientFactory {
    static createClient(config, providerModels) {
        let chatModel;
        const apiKey = config.apiKeyEnvVar ? process.env[config.apiKeyEnvVar] : undefined;
        const temperature = config.temperature ?? 0.7; // Default temperature
        const providerKey = config.provider.toLowerCase();
        // --- Determine the model to use ---
        let modelToUse = config.model; // Start with the model specified in servers.json
        if (!modelToUse) {
            // If no model specified in servers.json, try getting the default from provider_models.toml
            const suggestedModels = providerModels[providerKey]?.models;
            if (suggestedModels && suggestedModels.length > 0) {
                modelToUse = suggestedModels[0]; // Use the first model as default
                console.log(`No model specified for provider "${config.provider}", using default from suggestions: "${modelToUse}"`);
            }
            else {
                // No model in servers.json AND no suggestions found in TOML
                throw new Error(`AI model must be specified for provider "${config.provider}" in servers.json or provider_models.toml`);
            }
        }
        // --- End Model Determination ---
        console.log(`Creating AI client for provider: ${providerKey}, model: ${modelToUse}`);
        switch (providerKey) {
            case 'openai':
                chatModel = new ChatOpenAI({
                    modelName: modelToUse,
                    temperature: temperature,
                    openAIApiKey: apiKey, // apiKey is optional, LangChain checks OPENAI_API_KEY by default
                });
                break;
            case 'anthropic':
                chatModel = new ChatAnthropic({
                    modelName: modelToUse,
                    temperature: temperature,
                    anthropicApiKey: apiKey, // apiKey is optional, LangChain checks ANTHROPIC_API_KEY by default
                });
                break;
            case 'google-genai':
            case 'google': // Allow alias
                chatModel = new ChatGoogleGenerativeAI({
                    modelName: modelToUse,
                    temperature: temperature,
                    apiKey: apiKey, // apiKey is optional, LangChain checks GOOGLE_API_KEY by default
                });
                break;
            case 'mistralai':
            case 'mistral': // Allow alias
                chatModel = new ChatMistralAI({
                    modelName: modelToUse,
                    temperature: temperature,
                    apiKey: apiKey, // apiKey is optional, LangChain checks MISTRAL_API_KEY by default
                });
                break;
            case 'fireworks':
                chatModel = new ChatFireworks({
                    modelName: modelToUse,
                    temperature: temperature,
                    fireworksApiKey: apiKey, // LangChain checks FIREWORKS_API_KEY by default
                });
                break;
            // Add cases for other providers (Groq, Cohere, Ollama, etc.) here
            // Example:
            // case 'groq':
            //   chatModel = new ChatGroq({ model: modelToUse, temperature: temperature, apiKey: apiKey });
            //   break;
            default:
                throw new Error(`Unsupported AI provider: ${config.provider}`);
        }
        if (!chatModel) {
            throw new Error(`Failed to initialize chat model for provider: ${config.provider}`);
        }
        // Pass the actual model being used for identification purposes
        return new LangchainClient(chatModel, modelToUse);
    }
}
//# sourceMappingURL=AiClientFactory.js.map