import type { Client } from '@modelcontextprotocol/sdk/client/index.js';
import type { Transport } from '@modelcontextprotocol/sdk/shared/transport.js';
import type { StdioServerParameters } from '@modelcontextprotocol/sdk/client/stdio.js';
import type { Tool } from '@modelcontextprotocol/sdk/types.js';
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
    provider: string;
    model?: string;
    apiKeyEnvVar?: string;
    apiKey?: string;
    temperature?: number;
}
/**
 * Structure of the servers.json file (or main config)
 */
export interface ConfigFileStructure {
    mcpServers: Record<string, StdioServerConfig>;
}
/**
 * Structure for the ai_config.json file
 */
export interface AiConfigFileStructure {
    defaultProvider?: string;
    providers: Record<string, AiProviderConfig>;
}
/**
 * Server connection details
 */
export interface ServerConnection {
    client: Client;
    transport: Transport;
    tools?: Tool[];
    isConnected: boolean;
    config: StdioServerConfig;
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
    toolResult?: any;
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
