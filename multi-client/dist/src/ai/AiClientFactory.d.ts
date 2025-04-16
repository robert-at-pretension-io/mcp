import type { Tool } from '@modelcontextprotocol/sdk/types.js';
import type { AiProviderConfig, ProviderModelsStructure } from '../types.js';
import type { IAiClient } from './IAiClient.js';
/**
 * Custom error for missing API keys that require user prompting.
 */
export declare class MissingApiKeyError extends Error {
    providerName: string;
    apiKeyEnvVar: string;
    constructor(providerName: string, apiKeyEnvVar: string);
}
export declare class AiClientFactory {
    static createClient(config: AiProviderConfig, providerModels: ProviderModelsStructure, availableTools: Tool[]): IAiClient;
}
