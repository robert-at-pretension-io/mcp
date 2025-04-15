import type { Client } from '@modelcontextprotocol/sdk/client/index.js';
import type { Transport } from '@modelcontextprotocol/sdk/shared/transport.js';
import type { StdioServerParameters } from '@modelcontextprotocol/sdk/client/stdio.js';
import type { Tool } from '@modelcontextprotocol/sdk/types.js';

/**
 * Configuration for a single stdio server
 */
export interface StdioServerConfig extends Omit<StdioServerParameters, 'env'> {
  env?: Record<string, string>;
}

/**
 * Structure of the servers.json file
 */
export interface ConfigFileStructure {
  mcpServers: Record<string, StdioServerConfig>;
  timeouts?: {
    request: number;
    tool: number;
  };
}

/**
 * Server connection details
 */
export interface ServerConnection {
  client: Client;
  transport: Transport;
  tools?: Tool[];
  isConnected: boolean;
}

/**
 * Tool execution options
 */
export interface ToolExecutionOptions {
  timeout?: number;
  showProgress?: boolean;
}

/**
 * Result of a tool execution
 */
export interface ToolExecutionResult {
  serverName: string;
  toolName: string;
  executionTime: number;
  toolResult?: any;  // The actual result
  isError?: boolean;
  errorMessage?: string;
}

/**
 * REPL command definition
 */
export interface Command {
  name: string;
  description: string;
  execute: (args: string[]) => Promise<string | void>;
}
