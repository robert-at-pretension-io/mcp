import { ConfigFileStructure, ToolExecutionOptions, ToolExecutionResult, StdioServerConfig } from './types.js';
import type { Tool } from '@modelcontextprotocol/sdk/types.js';
/**
 * Manages connections to multiple MCP servers
 */
export declare class ServerManager {
    private servers;
    private config;
    private defaultToolTimeout;
    constructor(config: ConfigFileStructure);
    /**
     * Connect to all servers defined in the configuration
     */
    connectAll(): Promise<string[]>;
    /**
     * Connect to a specific server
     */
    connectToServer(serverName: string, serverConfig: StdioServerConfig): Promise<string>;
    /**
     * List names of servers currently marked as connected.
     */
    getConnectedServers(): string[];
    /**
     * Get the status of all configured servers.
     * @returns Record mapping server name to its status ('connected', 'disconnected', 'error', 'connecting').
     */
    getServerStatuses(): Record<string, {
        status: 'connected' | 'disconnected' | 'error' | 'connecting';
        error?: string;
    }>;
    /**
     * Attempts to reconnect to all servers currently marked as disconnected or errored.
     */
    retryAllFailedConnections(): Promise<string[]>;
    /**
     * List all tools for a specific server
     */
    getServerTools(serverName: string): Tool[];
    /**
     * Get all tools from all connected servers.
     */
    getAllTools(): Promise<Tool[]>;
    /**
     * Execute a tool on a specific server
     */
    executeTool(serverName: string, toolName: string, args: Record<string, any>, options?: ToolExecutionOptions): Promise<ToolExecutionResult>;
    /**
     * Find which server provides a specific tool
     */
    findToolProvider(toolName: string): string | null;
    /**
     * Close all server connections
     */
    closeAll(): Promise<void>;
}
