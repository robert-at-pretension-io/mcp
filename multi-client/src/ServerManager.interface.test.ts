import { ServerManager } from './ServerManager.js';
import type { StdioServerConfig } from './types.js';

// a simple config for testing interface methods
const config = { mcpServers: { s1: {}, s2: {} } } as any;

describe('ServerManager interface existence', () => {
  let sm: ServerManager;
  beforeEach(() => {
    sm = new ServerManager(config);
  });

  it('has connectAll method', () => {
    expect(typeof sm.connectAll).toBe('function');
  });

  it('has getConnectedServers method', () => {
    expect(typeof sm.getConnectedServers).toBe('function');
  });

  it('getServerStatuses returns an object with server keys', () => {
    const statuses = sm.getServerStatuses();
    expect(statuses).toHaveProperty('s1');
    expect(statuses).toHaveProperty('s2');
    expect(statuses.s1).toHaveProperty('status');
  });

  it('retryAllFailedConnections method exists', () => {
    expect(typeof sm.retryAllFailedConnections).toBe('function');
  });
});