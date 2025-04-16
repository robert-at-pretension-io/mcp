import {
  SystemMessage,
  HumanMessage,
  AIMessage,
  ToolMessage,
  createSystemMessage,
  createHumanMessage,
  createAiMessage,
  createToolMessage
} from './Message.js';

describe('SystemMessage and HumanMessage', () => {
  it('createSystemMessage produces SystemMessage with correct content', () => {
    const msg = createSystemMessage('sys');
    expect(msg._getType()).toBe('system');
    expect(msg.content).toBe('sys');
  });

  it('createHumanMessage produces HumanMessage with correct content', () => {
    const msg = createHumanMessage('hi');
    expect(msg._getType()).toBe('human');
    expect(msg.content).toBe('hi');
  });
});

describe('AIMessage', () => {
  it('defaults hasToolCalls to false', () => {
    const msg = createAiMessage('reply');
    expect(msg._getType()).toBe('ai');
    expect(msg.hasToolCalls).toBe(false);
    expect(msg.pendingToolCalls).toBe(false);
  });

  it('sets hasToolCalls and pendingToolCalls flags', () => {
    const msg = createAiMessage('reply', { hasToolCalls: true, pendingToolCalls: true });
    expect(msg.hasToolCalls).toBe(true);
    expect(msg.pendingToolCalls).toBe(true);
  });
});

describe('ToolMessage', () => {
  it('creates ToolMessage with correct data', () => {
    const msg = createToolMessage('res', 'id123', 'toolX');
    expect(msg._getType()).toBe('tool');
    // @ts-ignore
    expect(msg.tool_call_id).toBe('id123');
    expect(msg.content).toBe('res');
  });
});