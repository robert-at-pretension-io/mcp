import { ChatOpenAI } from '@langchain/openai';
import { ChatAnthropic } from '@langchain/anthropic';
import { ChatGoogleGenerativeAI } from '@langchain/google-genai';
import { ChatMistralAI } from '@langchain/mistralai';
import { ChatFireworks } from '@langchain/community/chat_models/fireworks';
import { convertToLangChainTool } from '../utils/toolConverter.js';
import { LangchainClient } from './LangchainClient.js';
/**
 * Custom error for missing API keys that require user prompting.
 */
export class MissingApiKeyError extends Error {
    providerName;
    apiKeyEnvVar;
    constructor(providerName, apiKeyEnvVar) {
        super(`API key environment variable "${apiKeyEnvVar}" for provider "${providerName}" is not set.`);
        this.name = 'MissingApiKeyError';
        this.providerName = providerName;
        this.apiKeyEnvVar = apiKeyEnvVar;
        // Set the prototype explicitly.
        Object.setPrototypeOf(this, MissingApiKeyError.prototype);
    }
}
export class AiClientFactory {
    static createClient(config, providerModels, availableTools // Accept the list of available MCP tools
    ) {
        let chatModel;
        let apiKeyToUse = undefined;
        // Use temperature from config if provided; otherwise, do not set (use model default)
        const temperature = config.temperature;
        const providerKey = config.provider.toLowerCase();
        // --- Determine API Key ---
        // 1. Check for direct apiKey in config
        if (config.apiKey) {
            apiKeyToUse = config.apiKey;
            console.log(`Using direct API key from config for provider "${providerKey}".`);
        }
        // 2. If no direct key, check environment variable specified by apiKeyEnvVar
        else if (config.apiKeyEnvVar) {
            apiKeyToUse = process.env[config.apiKeyEnvVar];
            if (apiKeyToUse) {
                console.log(`Using API key from environment variable "${config.apiKeyEnvVar}" for provider "${providerKey}".`);
            }
            else {
                // Environment variable specified but not set - throw specific error
                throw new MissingApiKeyError(config.provider, config.apiKeyEnvVar);
            }
        }
        // 3. If neither is set, some providers might work if LangChain has internal defaults (like checking OPENAI_API_KEY),
        // but we'll rely on the specific config for clarity. If no key source is defined, error out later if the provider requires one.
        // We pass `undefined` to the LangChain constructor, letting it handle its default env var checks if applicable.
        else {
            // No API key source specified, try to infer based on provider
            const envVarMap = {
                'openai': 'OPENAI_API_KEY',
                'anthropic': 'ANTHROPIC_API_KEY',
                'google-genai': 'GEMINI_API_KEY',
                'mistralai': 'MISTRAL_API_KEY',
                'fireworks': 'FIREWORKS_API_KEY',
                'openrouter': 'OPENROUTER_API_KEY' // Added OpenRouter default env var
            };
            const defaultEnvVar = envVarMap[providerKey];
            if (defaultEnvVar && process.env[defaultEnvVar]) {
                apiKeyToUse = process.env[defaultEnvVar];
                console.log(`Using API key from default environment variable "${defaultEnvVar}" for provider "${providerKey}".`);
            }
            else if (defaultEnvVar) {
                throw new MissingApiKeyError(config.provider, defaultEnvVar);
            }
        }
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
            case 'openai': {
                // Build options, include temperature only if specified
                const options = {
                    modelName: modelToUse,
                    openAIApiKey: apiKeyToUse,
                };
                if (temperature !== undefined)
                    options.temperature = temperature;
                chatModel = new ChatOpenAI(options);
                break;
            }
            case 'anthropic': {
                const options = {
                    modelName: modelToUse,
                    anthropicApiKey: apiKeyToUse,
                };
                if (temperature !== undefined)
                    options.temperature = temperature;
                chatModel = new ChatAnthropic(options);
                break;
            }
            case 'google-genai':
            case 'google': { // Allow alias
                const options = {
                    modelName: modelToUse,
                    apiKey: apiKeyToUse,
                };
                if (temperature !== undefined)
                    options.temperature = temperature;
                chatModel = new ChatGoogleGenerativeAI(options);
                break;
            }
            case 'mistralai':
            case 'mistral': { // Allow alias
                const options = {
                    modelName: modelToUse,
                    apiKey: apiKeyToUse,
                };
                if (temperature !== undefined)
                    options.temperature = temperature;
                chatModel = new ChatMistralAI(options);
                break;
            }
            case 'fireworks': {
                const options = {
                    modelName: modelToUse,
                    fireworksApiKey: apiKeyToUse,
                };
                if (temperature !== undefined)
                    options.temperature = temperature;
                chatModel = new ChatFireworks(options);
                break;
            }
            case 'openrouter': {
                const options = {
                    modelName: modelToUse,
                    openAIApiKey: apiKeyToUse,
                    configuration: { baseURL: 'https://openrouter.ai/api/v1' },
                };
                if (temperature !== undefined)
                    options.temperature = temperature;
                chatModel = new ChatOpenAI(options);
                break;
            }
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
        // Pass the actual model being used and provider name for identification purposes
        // Convert MCP Tools to LangChain StructuredTools
        const langchainTools = availableTools.map(convertToLangChainTool);
        // Bind the tools to the chat model instance if the method exists
        // This ensures LangChain includes tool definitions in API calls when needed
        let modelForClient = chatModel;
        if (langchainTools.length > 0) { // Only attempt binding if tools exist
            if (typeof chatModel.bindTools === 'function') {
                modelForClient = chatModel.bindTools(langchainTools);
                console.log(`[AiClientFactory] Tools bound to model ${modelToUse}`);
            }
            else {
                // If tools are available but the model doesn't support binding, log a warning but continue.
                console.warn(`[AiClientFactory] Warning: Model ${modelToUse} does not support standard tool binding (bindTools), but tools were provided. Tool usage might rely on custom parsing (e.g., MCP delimiters).`);
                // Proceed with the original model; tool usage depends on ConversationManager's parsing
                modelForClient = chatModel;
            }
        }
        else {
            // No tools provided, proceed with the original model
            console.log(`[AiClientFactory] No tools provided. Proceeding without tool binding.`);
            modelForClient = chatModel; // Use the original model
        }
        // Pass the potentially tool-bound model to the LangchainClient
        return new LangchainClient(modelForClient, modelToUse, config.provider);
    }
}
//# sourceMappingURL=AiClientFactory.js.map