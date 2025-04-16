import { ConversationState, VerificationState } from './ConversationState.js';
import { SystemMessage, HumanMessage, AIMessage, ToolMessage } from './Message.js';

describe('ConversationState basic operations', () => {
  let state: ConversationState;
  beforeEach(() => {
    state = new ConversationState('init');
  });

  it('initial system prompt appears first', () => {
    state.addMessage(new HumanMessage('hello'));
    const msgs = state.getMessages();
    expect(msgs[0]._getType()).toBe('system');
    expect(msgs[0].content).toBe('init');
  });

  it('getHistoryWithoutSystemPrompt excludes system', () => {
    state.addMessage(new HumanMessage('hi'));
    const hist = state.getHistoryWithoutSystemPrompt();
    expect(hist.length).toBe(1);
    expect(hist[0]._getType()).toBe('human');
  });

  it('clearHistory resets history and turn but keeps system', () => {
    state.addMessage(new HumanMessage('one'));
    state.incrementTurn();
    state.clearHistory();
    expect(state.getHistoryWithoutSystemPrompt()).toEqual([]);
    expect(state.getCurrentTurn()).toBe(0);
    const msgs = state.getMessages();
    expect(msgs.length).toBe(1);
    expect(msgs[0]._getType()).toBe('system');
  });

  it('replaceHistory sets new messages', () => {
    state.replaceHistory([new HumanMessage('a'), new AIMessage('b')]);
    const hist = state.getHistoryWithoutSystemPrompt();
    expect(hist.map(m => m._getType())).toEqual(['human', 'ai']);
  });

  it('incrementTurn increases turn only on human messages', () => {
    expect(state.getCurrentTurn()).toBe(0);
    state.addMessage(new AIMessage('a'));
    expect(state.getCurrentTurn()).toBe(0);
    state.addMessage(new HumanMessage('b'));
    expect(state.getCurrentTurn()).toBe(1);
  });
});

describe('ConversationState verification sequence', () => {
  let state: ConversationState;
  beforeEach(() => {
    state = new ConversationState();
    state.addMessage(new HumanMessage('ask')); // turn 1
    state.addMessage(new AIMessage('resp'));
    state.addMessage(new HumanMessage('ask2')); // turn 2
    state.addMessage(new AIMessage('resp2'));
    state.setVerificationState('orig', 'crit');
  });

  it('getVerificationState returns set state', () => {
    const vs = state.getVerificationState();
    expect(vs).not.toBeNull();
    expect(vs?.originalRequest).toBe('orig');
    expect(vs?.criteria).toBe('crit');
  });

  it('getRelevantSequenceForVerification returns formatted string', () => {
    const seq = state.getRelevantSequenceForVerification();
    expect(seq).toContain('User: ask');
    expect(seq).toContain('Assistant: resp');
  });

  it('getRelevantSequenceForVerification returns empty if no verification', () => {
    const s2 = new ConversationState();
    expect(s2.getRelevantSequenceForVerification()).toBe('');
  });

  it('removeLastMessageIfPendingAiToolCall pops pending tool call', () => {
    const m = new AIMessage('foo', { pendingToolCalls: true });
    state.addMessage(m);
    state.removeLastMessageIfPendingAiToolCall();
    const hist = state.getHistoryWithoutSystemPrompt();
    expect(hist.find(msg => msg === m)).toBeUndefined();
  });

  it('removeLastMessageIfPendingAiToolCall does nothing for non-pending', () => {
    const m = new AIMessage('foo');
    state.addMessage(m);
    state.removeLastMessageIfPendingAiToolCall();
    const hist = state.getHistoryWithoutSystemPrompt();
    expect(hist.find(msg => msg === m)).toBeDefined();
  });
});