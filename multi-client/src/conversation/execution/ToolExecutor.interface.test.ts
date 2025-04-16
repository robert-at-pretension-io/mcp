import { ToolExecutor } from './ToolExecutor.js';
describe('ToolExecutor interface', () => {
  it('has executeToolCalls method', () => {
    expect(typeof ToolExecutor.prototype.executeToolCalls).toBe('function');
  });
});