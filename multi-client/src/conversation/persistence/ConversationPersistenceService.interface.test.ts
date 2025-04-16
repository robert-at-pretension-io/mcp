import { ConversationPersistenceService } from './ConversationPersistenceService.js';
describe('ConversationPersistenceService interface', () => {
  it('has saveConversation method', () => {
    expect(typeof ConversationPersistenceService.prototype.saveConversation).toBe('function');
  });

  it('has loadConversation method', () => {
    expect(typeof ConversationPersistenceService.prototype.loadConversation).toBe('function');
  });
});