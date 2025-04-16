import { AiClientFactory, MissingApiKeyError } from './AiClientFactory.js';
import type { AiProviderConfig, ProviderModelsStructure } from '../types.js';

describe('MissingApiKeyError', () => {
  it('sets message and properties correctly', () => {
    const err = new MissingApiKeyError('TestProvider', 'TEST_KEY_VAR');
    expect(err).toBeInstanceOf(Error);
    expect(err.name).toBe('MissingApiKeyError');
    expect(err.providerName).toBe('TestProvider');
    expect(err.apiKeyEnvVar).toBe('TEST_KEY_VAR');
    expect(err.message).toContain('TEST_KEY_VAR');
  });
});

describe('AiClientFactory.createClient', () => {
  const emptyTools: any[] = [];
  const emptyProviders: ProviderModelsStructure = {};

  afterEach(() => {
    // Clean up environment variables
    delete process.env.OPENAI_API_KEY;
    delete process.env.TEST_ENV_VAR;
  });

  it('uses direct apiKey and returns a client with correct model and provider', () => {
    const config: AiProviderConfig = {
      provider: 'openai',
      apiKey: 'direct-key',
      model: 'direct-model',
      temperature: 0.5,
    };
    const client = AiClientFactory.createClient(config, emptyProviders, emptyTools);
    expect(client.getModelName()).toBe('direct-model');
    expect(client.getProvider?.()).toBe('openai');
  });

  it('throws MissingApiKeyError when env var is specified but not set', () => {
    const config: AiProviderConfig = {
      provider: 'openai',
      apiKeyEnvVar: 'TEST_ENV_VAR',
      model: 'model-x',
    };
    expect(() => AiClientFactory.createClient(config, emptyProviders, emptyTools)).toThrow(MissingApiKeyError);
  });

  it('uses default env var for provider when none specified in config', () => {
    process.env.OPENAI_API_KEY = 'env-key';
    const config: AiProviderConfig = {
      provider: 'openai',
      model: 'env-model',
    };
    const client = AiClientFactory.createClient(config, emptyProviders, emptyTools);
    expect(client.getModelName()).toBe('env-model');
    expect(client.getProvider?.()).toBe('openai');
  });

  it('uses suggested model when config.model is undefined', () => {
    process.env.OPENAI_API_KEY = 'env-key';
    const providers: ProviderModelsStructure = { openai: { models: ['suggested-model'] } };
    const config: AiProviderConfig = {
      provider: 'openai',
      apiKeyEnvVar: 'OPENAI_API_KEY',
    };
    const client = AiClientFactory.createClient(config, providers, emptyTools);
    expect(client.getModelName()).toBe('suggested-model');
  });

  it('throws Error when no model and no suggestions', () => {
    process.env.OPENAI_API_KEY = 'env-key';
    const config: AiProviderConfig = {
      provider: 'openai',
      apiKeyEnvVar: 'OPENAI_API_KEY',
    };
    const providers: ProviderModelsStructure = {};
    expect(() => AiClientFactory.createClient(config, providers, emptyTools)).toThrow(/AI model must be specified/);
  });
});