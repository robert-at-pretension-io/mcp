import type { AiProviderConfig, ProviderModelsStructure } from '../types.js';
import type { IAiClient } from './IAiClient.js';
export declare class AiClientFactory {
    static createClient(config: AiProviderConfig, providerModels: ProviderModelsStructure): IAiClient;
}
