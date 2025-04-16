import { AiClientFactory, MissingApiKeyError } from './AiClientFactory.js';
import type { AiProviderConfig, ProviderModelsStructure } from '../types.js';

describe('AiClientFactory provider mappings', () => {
  const emptyProviders: ProviderModelsStructure = {};
  const emptyTools: any[] = [];
  afterEach(() => {
    delete process.env.ANTHROPIC_API_KEY;
    delete process.env.GEMINI_API_KEY;
    delete process.env.MISTRAL_API_KEY;
    delete process.env.FIREWORKS_API_KEY;
    delete process.env.OPENAI_API_KEY;
  });

  it('creates anthropic client with default env var', () => {
    process.env.ANTHROPIC_API_KEY = 'anthro-key';
    const config: AiProviderConfig = { provider: 'anthropic', model: 'm1' };
    const client = AiClientFactory.createClient(config, emptyProviders, emptyTools);
    expect(client.getProvider?.()).toBe('anthropic');
    expect(client.getModelName()).toBe('m1');
  });

  it('creates google-genai client with default env var', () => {
    process.env.GEMINI_API_KEY = 'g-key';
    const config: AiProviderConfig = { provider: 'google-genai', model: 'g2' };
    const client = AiClientFactory.createClient(config, emptyProviders, emptyTools);
    expect(client.getProvider?.()).toBe('google-genai');
    expect(client.getModelName()).toBe('g2');
  });

  it('throws error for google alias due to missing GOOGLE_API_KEY', () => {
    process.env.GEMINI_API_KEY = 'g-key2';
    const config: AiProviderConfig = { provider: 'google', model: 'g3' };
    expect(() => AiClientFactory.createClient(config, emptyProviders, emptyTools)).toThrow(/Please set an API key/);
  });

  it('creates mistralai client with default env var', () => {
    process.env.MISTRAL_API_KEY = 'm-key';
    const config: AiProviderConfig = { provider: 'mistralai', model: 'm2' };
    const client = AiClientFactory.createClient(config, emptyProviders, emptyTools);
    expect(client.getProvider?.()).toBe('mistralai');
    expect(client.getModelName()).toBe('m2');
  });

  it('creates fireworks client with default env var', () => {
    process.env.FIREWORKS_API_KEY = 'f-key';
    const config: AiProviderConfig = { provider: 'fireworks', model: 'f1' };
    const client = AiClientFactory.createClient(config, emptyProviders, emptyTools);
    expect(client.getProvider?.()).toBe('fireworks');
    expect(client.getModelName()).toBe('f1');
  });

  it('creates openrouter client with default env var', () => {
    process.env.OPENAI_API_KEY = 'o-key';
    const config: AiProviderConfig = { provider: 'openrouter', model: 'o1' };
    const client = AiClientFactory.createClient(config, emptyProviders, emptyTools);
    expect(client.getProvider?.()).toBe('openrouter');
    expect(client.getModelName()).toBe('o1');
  });

  it('throws for unsupported provider', () => {
    const config: AiProviderConfig = { provider: 'unknown', model: 'x' };
    expect(() => AiClientFactory.createClient(config, emptyProviders, emptyTools)).toThrow(/Unsupported AI provider/);
  });
});