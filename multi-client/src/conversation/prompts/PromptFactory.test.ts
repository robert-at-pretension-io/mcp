import { PromptFactory } from './PromptFactory.js';
import type { Tool } from '@modelcontextprotocol/sdk/types.js';

describe('PromptFactory.fill functions', () => {
  it('fillVerificationCriteriaPrompt replaces user_request', () => {
    const prompt = PromptFactory.fillVerificationCriteriaPrompt('Do X');
    expect(prompt).toContain('User Request:');
    expect(prompt).toContain('Do X');
  });

  it('fillVerificationPrompt replaces all placeholders', () => {
    const filled = PromptFactory.fillVerificationPrompt('Req', 'Crit', 'Seq');
    expect(filled).toContain('Req');
    expect(filled).toContain('Crit');
    expect(filled).toContain('Seq');
    // Should start with '{'
    expect(filled.startsWith('You are a strict evaluator')).toBe(true);
  });

  it('fillVerificationFailurePrompt replaces feedback', () => {
    const filled = PromptFactory.fillVerificationFailurePrompt('oops');
    expect(filled).toContain('oops');
    expect(filled.startsWith('Your previous response failed verification')).toBe(true);
  });

  it('fillCompactionPrompt replaces history_string and retains template prefix', () => {
    const filled = PromptFactory.fillCompactionPrompt('HIST');
    // Should contain the inserted history_string
    expect(filled).toContain('HIST');
    // Should begin with the summary prompt
    expect(filled.startsWith('You are an expert conversation summarizer')).toBe(true);
  });

  it('fillInvalidToolFormatPrompt replaces invalid_content', () => {
    const filled = PromptFactory.fillInvalidToolFormatPrompt('BAD');
    expect(filled).toContain('BAD');
    expect(filled.startsWith('Correction Request')).toBe(true);
  });
});

describe('PromptFactory.createToolSystemPrompt', () => {
  it('handles no tools gracefully', () => {
    const prompt = PromptFactory.createToolSystemPrompt([]);
    expect(prompt).toContain('No tools are currently available');
  });

  it('includes tool descriptions and schemas', () => {
    const tools: Tool[] = [{
      name: 't1',
      description: 'desc1',
      input_schema: JSON.stringify({ properties: { a: { type: 'string' } } })
    } as any];
    const prompt = PromptFactory.createToolSystemPrompt(tools);
    expect(prompt).toContain('## t1');
    expect(prompt).toContain('desc1');
    expect(prompt).toContain('"type": "string"');
    expect(prompt).toMatch(/<<<TOOL_CALL>>>/);
  });
});