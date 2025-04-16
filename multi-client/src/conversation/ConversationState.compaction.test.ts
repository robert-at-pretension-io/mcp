import { ConversationState } from './ConversationState.js';
import { SystemMessage, HumanMessage, AIMessage } from './Message.js';

describe('ConversationState.compactHistory', () => {
  const compactionTemplate = 'SUM:{history_string}';
  let state: ConversationState;
  beforeEach(() => {
    state = new ConversationState('init');
  });

  it('skips compaction when history too small', async () => {
    state.addMessage(new HumanMessage('a'));
    const aiClient = { generateResponse: jest.fn().mockResolvedValue('ignored') };
    await state.compactHistory(compactionTemplate, aiClient as any);
    // No change: history length remains 1
    expect(state.getHistoryWithoutSystemPrompt().length).toBe(1);
    expect(aiClient.generateResponse).not.toHaveBeenCalled();
  });

  it('compacts history when threshold reached', async () => {
    // Fill with 14 history messages (threshold = 10+4)
    for (let i = 0; i < 14; i++) {
      const msg = i % 2 === 0 ? new HumanMessage(`h${i}`) : new AIMessage(`a${i}`);
      state.addMessage(msg);
    }
    const aiClient = { generateResponse: jest.fn().mockResolvedValue('SUMMARY') };
    await state.compactHistory(compactionTemplate, aiClient as any);
    // After compaction, history length should be last 10 messages
    expect(state.getHistoryWithoutSystemPrompt().length).toBe(10);
    // System prompt should include summary and original init
    const messages = state.getMessages();
    expect(messages[0]._getType()).toBe('system');
    expect(messages[0].content).toContain('[Previous conversation summary:');
    expect(messages[0].content).toContain('SUMMARY');
    expect(messages[0].content).toContain('init');
    // AI client was called with summary prompt
    expect(aiClient.generateResponse).toHaveBeenCalled();
  });

  it('does not alter history on aiClient error', async () => {
    for (let i = 0; i < 14; i++) {
      state.addMessage(new HumanMessage(`x${i}`));
    }
    const aiClient = { generateResponse: jest.fn().mockRejectedValue(new Error('fail')) };
    // Capture initial history copy
    const beforeHist = [...state.getHistoryWithoutSystemPrompt()];
    await state.compactHistory(compactionTemplate, aiClient as any);
    // History unchanged
    expect(state.getHistoryWithoutSystemPrompt()).toEqual(beforeHist);
  });
});