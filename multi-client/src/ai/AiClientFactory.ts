import { ChatOpenAI } from '@langchain/openai';
import { ChatAnthropic } from '@langchain/anthropic';
import { ChatGoogleGenerativeAI } from '@langchain/google-genai';
import { ChatMistralAI } from '@langchain/mistralai';
import { ChatFireworks } from '@langchain/community/chat_models/fireworks';
// Import other necessary chat models (e.g., Groq, Cohere) if needed
// Removing TogetherAI for now due to import issues

import type { BaseChatModel, BaseChatModelCallOptions } from '@langchain/core/language_models/chat_models';
// Import necessary types for RunnableInterface and its inputs/outputs
import type { RunnableInterface } from "@langchain/core/runnables";
import type { BaseLanguageModelInput } from "@langchain/core/language_models/base";
import type { BaseMessageChunk } from "@langchain/core/messages";

import type { Tool } from '@modelcontextprotocol/sdk/types.js'; // Import MCP Tool type
import type { StructuredToolInterface } from '@langchain/core/tools'; // Import LangChain Tool type
import { convertToLangChainTool } from '../utils/toolConverter.js'; // We'll create this utility
import type { AiProviderConfig, ProviderModelsStructure } from '../types.js';
import { LangchainClient } from './LangchainClient.js';
import type { IAiClient } from './IAiClient.js';

/**
 * Custom error for missing API keys that require user prompting.
 */
export class MissingApiKeyError extends Error {
  public providerName: string;
  public apiKeyEnvVar: string;

  constructor(providerName: string, apiKeyEnvVar: string) {
    super(`API key environment variable "${apiKeyEnvVar}" for provider "${providerName}" is not set.`);
    this.name = 'MissingApiKeyError';
    this.providerName = providerName;
    this.apiKeyEnvVar = apiKeyEnvVar;
    // Set the prototype explicitly.
    Object.setPrototypeOf(this, MissingApiKeyError.prototype);
  }
}


export class AiClientFactory {
  static createClient(
    config: AiProviderConfig,
    providerModels: ProviderModelsStructure,
    availableTools: Tool[] // Accept the list of available MCP tools
  ): IAiClient {
    let chatModel: BaseChatModel;
    let apiKeyToUse: string | undefined = undefined;
    const temperature = config.temperature ?? 0.7; // Default temperature
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
      } else {
        // Environment variable specified but not set - throw specific error
        throw new MissingApiKeyError(config.provider, config.apiKeyEnvVar);
      }
    }
    // 3. If neither is set, some providers might work if LangChain has internal defaults (like checking OPENAI_API_KEY),
    // but we'll rely on the specific config for clarity. If no key source is defined, error out later if the provider requires one.
    // We pass `undefined` to the LangChain constructor, letting it handle its default env var checks if applicable.
    else {
      // No API key source specified, try to infer based on provider
      const envVarMap: Record<string, string> = {
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
      } else if (defaultEnvVar) {
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
      } else {
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
          openAIApiKey: apiKeyToUse, // Pass the determined key
        });
        break;
      case 'anthropic':
        chatModel = new ChatAnthropic({
          modelName: modelToUse,
          temperature: temperature,
          anthropicApiKey: apiKeyToUse, // Pass the determined key
        });
        break;
      case 'google-genai':
      case 'google': // Allow alias
        chatModel = new ChatGoogleGenerativeAI({
          modelName: modelToUse,
          temperature: temperature,
          apiKey: apiKeyToUse, // Pass the determined key
        });
        break;
      case 'mistralai':
      case 'mistral': // Allow alias
        chatModel = new ChatMistralAI({
          modelName: modelToUse,
          temperature: temperature,
          apiKey: apiKeyToUse, // Pass the determined key
        });
        break;
      case 'fireworks':
        chatModel = new ChatFireworks({
          modelName: modelToUse,
          temperature: temperature,
          fireworksApiKey: apiKeyToUse, // Pass the determined key
        });
        break;
      case 'openrouter':
        chatModel = new ChatOpenAI({
          modelName: modelToUse,
          temperature: temperature,
          openAIApiKey: apiKeyToUse, // Pass the determined key
          configuration: {
            baseURL: 'https://openrouter.ai/api/v1',
          },
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

    // Pass the actual model being used and provider name for identification purposes
    // Convert MCP Tools to LangChain StructuredTools
    const langchainTools: StructuredToolInterface[] = availableTools.map(convertToLangChainTool);

    // Bind the tools to the chat model instance if the method exists
    // This ensures LangChain includes tool definitions in API calls when needed
    let modelForClient: BaseChatModel | RunnableInterface<BaseLanguageModelInput, BaseMessageChunk> = chatModel;
    if (typeof (chatModel as any).bindTools === 'function') {
        modelForClient = (chatModel as any).bindTools(langchainTools);
        console.log(`[AiClientFactory] Tools bound to model ${modelToUse}`);
    } else if (langchainTools.length > 0) {
        // If tools are available but the model doesn't support binding, it's an issue.
        console.error(`[AiClientFactory] Error: Model ${modelToUse} does not support bindTools, but tools were provided. Standard tool calling will not work.`);
        throw new Error(`Model ${modelToUse} does not support bindTools, cannot use tools effectively.`);
    } else {
        // No tools provided, or model doesn't support binding (and no tools needed) - proceed with original model
        console.log(`[AiClientFactory] No tools provided or model ${modelToUse} does not support bindTools. Proceeding without tool binding.`);
        modelForClient = chatModel; // Use the original model
    }

    // Pass the potentially tool-bound model to the LangchainClient
    return new LangchainClient(modelForClient, modelToUse, config.provider);
  }
}
