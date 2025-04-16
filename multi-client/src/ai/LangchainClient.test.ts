import { LangchainClient } from './LangchainClient.js';
import type { ConversationMessage } from '../conversation/Message.js';

// Stub for the RunnableInterface
class StubModel {
  constructor(private response: any) {}
  async invoke(_messages: ConversationMessage[]): Promise<{ content: any }> {
    return { content: this.response };
  }
}

describe('LangchainClient', () => {
  it('returns string content when response.content is string', async () => {
    const stub = new StubModel('hello world');
    const client = new LangchainClient(stub as any, 'model-1', 'provider-1');
    const result = await client.generateResponse([]);
    expect(result).toBe('hello world');
  });

  it('concatenates text items when response.content is array of text chunks', async () => {
    const chunks = [
      { type: 'text', text: 'part1' },
      { type: 'text', text: 'part2' },
    ];
    const stub = new StubModel(chunks);
    const client = new LangchainClient(stub as any, 'model-2', 'provider-2');
    const result = await client.generateResponse([]);
    expect(result).toBe('part1\npart2');
  });

  it('handles empty array response', async () => {
    const stub = new StubModel([]);
    const client = new LangchainClient(stub as any, 'model-3', 'provider-3');
    const result = await client.generateResponse([]);
    expect(result).toBe('[AI response was empty]');
  });

  it('handles non-text array elements', async () => {
    const data = [{ foo: 'bar' }];
    const stub = new StubModel(data);
    const client = new LangchainClient(stub as any, 'model-4', 'provider-4');
    const result = await client.generateResponse([]);
    expect(result).toMatch(/AI response contained non-text elements/);
  });

  it('stringifies non-string, non-array response.content', async () => {
    const stub = new StubModel(12345);
    const client = new LangchainClient(stub as any, 'model-5', 'provider-5');
    const result = await client.generateResponse([]);
    expect(result).toBe('12345');
  });

  it('getModelName and getProvider return correct values', () => {
    const stub = new StubModel('ok');
    const client = new LangchainClient(stub as any, 'model-name', 'prov-name');
    expect(client.getModelName()).toBe('model-name');
    expect(client.getProvider()).toBe('prov-name');
  });

  it('throws error when invoke rejects', async () => {
    const stub = { async invoke() { throw new Error('fail'); } };
    const client = new LangchainClient(stub as any, 'model-err', 'prov-err');
    await expect(client.generateResponse([])).rejects.toThrow('fail');
  });
});