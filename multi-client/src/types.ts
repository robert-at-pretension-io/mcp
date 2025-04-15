import type { Client } from '@modelcontextprotocol/sdk/client/index.js';
import type { Transport } from '@modelcontextprotocol/sdk/shared/transport.js';
import type { StdioServerParameters } from '@modelcontextprotocol/sdk/client/stdio.js';
import type { Tool } from '@modelcontextprotocol/sdk/types.js';
// Import LangChain message types
import type { BaseMessage } from '@langchain/core/messages';

/**
 * Configuration for a single stdio server
 */
export interface StdioServerConfig extends Omit<StdioServerParameters, 'env'> {
  env?: Record<string, string>;
}

/**
 * Configuration for an AI Provider
 */
export interface AiProviderConfig {
  provider: string; // e.g., "openai", "anthropic", "google-genai", "mistralai", "fireworks"
  model?: string; // Optional: e.g., "gpt-4o-mini", "claude-3-5-sonnet-20240620" - If omitted, uses default from TOML
  apiKeyEnvVar?: string; // Optional: Environment variable name for the API key (defaults based on provider)
  temperature?: number; // Optional: Model temperature
  // Add other provider-specific options if needed
}

/**
 * Structure of the servers.json file (or main config)
 */
export interface ConfigFileStructure {
  mcpServers: Record<string, StdioServerConfig>;
  timeouts?: {
    request: number;
    tool: number;
  };
  ai?: { // New section for AI configuration
    defaultProvider?: string; // Name of the default provider key below
    providers: Record<string, AiProviderConfig>; // Map of provider configurations
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
  config: StdioServerConfig; // Keep config for potential restarts
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

/**
 * Type alias for LangChain messages used in conversation state
 */
export type ConversationMessage = BaseMessage;


/**
 * Structure for the provider_models.toml file content
 */
export interface ProviderModelList {
    models: string[];
}

export type ProviderModelsStructure = Record<string, ProviderModelList>;
