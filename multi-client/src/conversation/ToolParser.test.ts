import { ToolParser, ParsedToolCall } from './ToolParser.js';

describe('ToolParser.containsToolCalls', () => {
  it('returns false when no delimiters present', () => {
    expect(ToolParser.containsToolCalls('hello')).toBe(false);
  });

  it('returns true when both delimiters present', () => {
    const text = '<<<TOOL_CALL>>>{}<<<END_TOOL_CALL>>>'; 
    expect(ToolParser.containsToolCalls(text)).toBe(true);
  });
});

describe('ToolParser.parseToolCalls', () => {
  it('parses a single valid tool call', () => {
    const json = JSON.stringify({ name: 't', arguments: { x: 1 } });
    const text = `prefix<<<TOOL_CALL>>>${json}<<<END_TOOL_CALL>>>suffix`;
    const calls = ToolParser.parseToolCalls(text);
    expect(calls).toHaveLength(1);
    const call = calls[0];
    expect(call.name).toBe('t');
    expect(call.arguments).toEqual({ x: 1 });
    expect(call.fullText).toContain(json);
  });

  it('parses multiple valid calls', () => {
    const call1 = JSON.stringify({ name: 'a', arguments: {} });
    const call2 = JSON.stringify({ name: 'b', arguments: { y: 2 } });
    const text = `<<<TOOL_CALL>>>${call1}<<<END_TOOL_CALL>>><<X>><<<TOOL_CALL>>>${call2}<<<END_TOOL_CALL>>>`;
    const calls = ToolParser.parseToolCalls(text);
    expect(calls.map(c => c.name)).toEqual(['a', 'b']);
  });

  it('skips invalid JSON', () => {
    const text = '<<<TOOL_CALL>>>notjson<<<END_TOOL_CALL>>>';
    const calls = ToolParser.parseToolCalls(text);
    expect(calls).toEqual([]);
  });

  it('skips when missing end delimiter', () => {
    const json = JSON.stringify({ name: 't', arguments: {} });
    const text = `<<<TOOL_CALL>>>${json}`;
    const calls = ToolParser.parseToolCalls(text);
    expect(calls).toEqual([]);
  });

  it('skips when missing name/arguments', () => {
    const json = JSON.stringify({ foo: 1 });
    const text = `<<<TOOL_CALL>>>${json}<<<END_TOOL_CALL>>>`;
    const calls = ToolParser.parseToolCalls(text);
    expect(calls).toEqual([]);
  });
});

describe('ToolParser.extractAndReplace', () => {
  it('replaces calls with placeholders', () => {
    const json = JSON.stringify({ name: 't', arguments: {} });
    const full = `Hello<<<TOOL_CALL>>>${json}<<<END_TOOL_CALL>>>Bye`;
    const { cleanText, toolCalls } = ToolParser.extractAndReplace(full);
    expect(cleanText).toContain('[Tool Call: t]');
    expect(toolCalls).toHaveLength(1);
    expect(toolCalls[0].name).toBe('t');
  });
});