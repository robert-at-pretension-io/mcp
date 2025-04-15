import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { StdioClientTransport } from '@modelcontextprotocol/sdk/client/stdio.js';
import { 
  ConfigFileStructure, 
  ServerConnection, 
  ToolExecutionOptions,
  ToolExecutionResult
} from './types.js';
import type { Implementation } from '@modelcontextprotocol/sdk/types.js';

/**
 * Manages connections to multiple MCP servers
 */
export class ServerManager {
  private servers: Record<string, ServerConnection> = {};
  private config: ConfigFileStructure;
  private defaultToolTimeout: number;
  
  constructor(config: ConfigFileStructure) {
    this.config = config;
    this.defaultToolTimeout = config.timeouts?.tool || 300;
  }

  /**
   * Connect to all servers defined in the configuration
   */
  async connectAll(): Promise<string[]> {
    const connectionPromises: Array<Promise<string>> = [];
    
    for (const [serverName, serverConfig] of Object.entries(this.config.mcpServers)) {
      connectionPromises.push(this.connectToServer(serverName, serverConfig));
    }
    
    const results = await Promise.allSettled(connectionPromises);
    
    // Filter for fulfilled promises and return their values
    return results
      .filter((result): result is PromiseFulfilledResult<string> => result.status === 'fulfilled')
      .map(result => result.value);
  }

  /**
   * Connect to a specific server
   */
  async connectToServer(serverName: string, serverConfig: any): Promise<string> {
    console.log(`[${serverName}] Attempting to connect...`);
    
    try {
      // Create transport
      const transport = new StdioClientTransport({
        command: serverConfig.command,
        args: serverConfig.args || [],
      });
      
      // Apply environment variables if defined
      if (serverConfig.env && Object.keys(serverConfig.env).length > 0) {
        Object.entries(serverConfig.env).forEach(([key, value]) => {
          process.env[key] = value as string;
        });
        console.log(`[${serverName}] Set environment variables: ${Object.keys(serverConfig.env).join(', ')}`);
      }
      
      // Set up transport error handlers
      transport.onerror = (error) => {
        console.error(`[${serverName}] Transport error:`, error.message);
        if (this.servers[serverName]) {
          this.servers[serverName].isConnected = false;
        }
      };
      
      transport.onclose = () => {
        console.log(`[${serverName}] Connection closed.`);
        if (this.servers[serverName]) {
          this.servers[serverName].isConnected = false;
        }
      };
      
      // Create client
      const clientInfo: Implementation = {
        name: `multi-client-${serverName.replace(/\s+/g, '-')}`,
        version: '1.0.0',
      };
      
      const client = new Client(clientInfo);
      
      // Connect client to transport
      await client.connect(transport);
      
      // Store connection
      this.servers[serverName] = {
        client,
        transport,
        isConnected: true
      };
      
      // Fetch and store tools
      const toolsResult = await client.listTools();
      this.servers[serverName].tools = toolsResult.tools;
      
      console.log(`[${serverName}] Successfully connected! Found ${toolsResult.tools.length} tools.`);
      return serverName;
    } catch (error) {
      console.error(`[${serverName}] Connection error:`, error instanceof Error ? error.message : String(error));
      throw new Error(`Failed to connect to server ${serverName}: ${error instanceof Error ? error.message : String(error)}`);
    }
  }

  /**
   * List all connected servers
   */
  getConnectedServers(): string[] {
    return Object.entries(this.servers)
      .filter(([_, connection]) => connection.isConnected)
      .map(([name]) => name);
  }

  /**
   * List all tools for a specific server
   */
  getServerTools(serverName: string) {
    const server = this.servers[serverName];
    if (!server || !server.isConnected) {
      throw new Error(`Server ${serverName} is not connected.`);
    }
    return server.tools || [];
  }

  /**
   * Execute a tool on a specific server
   */
  async executeTool(
    serverName: string, 
    toolName: string, 
    args: Record<string, any>,
    options: ToolExecutionOptions = {}
  ): Promise<ToolExecutionResult> {
    const startTime = Date.now();
    
    // Get server connection
    const server = this.servers[serverName];
    if (!server || !server.isConnected) {
      throw new Error(`Server ${serverName} is not connected.`);
    }
    
    // Set up timeout
    const timeout = options.timeout || this.defaultToolTimeout;
    
    // Show progress indicator if requested
    let progressInterval: NodeJS.Timeout | undefined;
    if (options.showProgress) {
      const spinner = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
      let i = 0;
      process.stdout.write(`\r[${serverName}] Executing tool '${toolName}'... ${spinner[i]} `);
      progressInterval = setInterval(() => {
        i = (i + 1) % spinner.length;
        process.stdout.write(`\r[${serverName}] Executing tool '${toolName}'... ${spinner[i]} `);
      }, 100);
    }
    
    try {
      // Execute with timeout
      const result = await Promise.race([
        server.client.callTool({
          name: toolName,
          parameters: args
        }),
        new Promise<never>((_, reject) => 
          setTimeout(() => reject(new Error(`Tool execution timed out after ${timeout}ms`)), timeout)
        )
      ]);
      
      // Return formatted result
      return {
        serverName,
        toolName,
        executionTime: Date.now() - startTime,
        toolResult: result,
        isError: false
      };
    } catch (error) {
      return {
        serverName,
        toolName,
        executionTime: Date.now() - startTime,
        isError: true,
        errorMessage: error instanceof Error ? error.message : String(error)
      };
    } finally {
      // Clear progress indicator if it was shown
      if (progressInterval) {
        clearInterval(progressInterval);
        if (options.showProgress) {
          process.stdout.write('\r' + ' '.repeat(80) + '\r'); // Clear the line
        }
      }
    }
  }

  /**
   * Find which server provides a specific tool
   */
  findToolProvider(toolName: string): string | null {
    for (const [serverName, connection] of Object.entries(this.servers)) {
      if (!connection.isConnected || !connection.tools) continue;
      
      const hasTool = connection.tools.some(tool => tool.name === toolName);
      if (hasTool) return serverName;
    }
    
    return null;
  }

  /**
   * Close all server connections
   */
  async closeAll(): Promise<void> {
    const closePromises = Object.entries(this.servers).map(async ([name, connection]) => {
      if (connection.isConnected) {
        console.log(`Closing connection to ${name}...`);
        try {
          await connection.transport.close();
          connection.isConnected = false;
        } catch (error) {
          console.error(`Error closing connection to ${name}:`, 
            error instanceof Error ? error.message : String(error));
        }
      }
    });
    
    await Promise.allSettled(closePromises);
  }
}
