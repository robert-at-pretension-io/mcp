import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { StdioClientTransport } from '@modelcontextprotocol/sdk/client/stdio.js';
/**
 * Manages connections to multiple MCP servers
 */
export class ServerManager {
    servers = {};
    config;
    defaultToolTimeout; // seconds
    constructor(config) {
        this.config = config;
        // Default timeout from config or 300 seconds (5 minutes)
        this.defaultToolTimeout = (config.timeouts?.tool ?? 300) * 1000; // Store in ms
    }
    /**
     * Connect to all servers defined in the configuration
     */
    async connectAll() {
        const connectionPromises = [];
        for (const [serverName, serverConfig] of Object.entries(this.config.mcpServers)) {
            connectionPromises.push(this.connectToServer(serverName, serverConfig));
        }
        const results = await Promise.allSettled(connectionPromises);
        // Filter for fulfilled promises and return their values
        return results
            .filter((result) => result.status === 'fulfilled')
            .map(result => result.value);
    }
    /**
     * Connect to a specific server
     */
    async connectToServer(serverName, serverConfig) {
        console.log(`[${serverName}] Attempting to connect...`);
        // Check if already connected or connection attempt in progress
        if (this.servers[serverName]?.isConnected) {
            console.log(`[${serverName}] Already connected.`);
            return serverName;
        }
        if (this.servers[serverName]) {
            console.log(`[${serverName}] Connection attempt already made (failed?). Skipping.`);
            // Or potentially retry logic here? For now, just skip.
            throw new Error(`Previous connection attempt failed for ${serverName}.`);
        }
        try {
            // Create transport
            const transport = new StdioClientTransport({
                command: serverConfig.command,
                args: serverConfig.args || [],
                // Apply environment variables if defined
                env: { ...Object.fromEntries(Object.entries(process.env).filter(([_, v]) => v !== undefined)), ...(serverConfig.env || {}) }
            });
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
            const clientInfo = {
                name: `mcp-multi-client-${serverName.replace(/\s+/g, '-')}`, // Consistent name prefix
                version: '1.0.0', // Use version from package.json?
            };
            const client = new Client(clientInfo);
            // Store connection attempt immediately
            this.servers[serverName] = {
                client,
                transport,
                isConnected: false, // Mark as not connected until successful
                config: serverConfig,
            };
            // Connect client to transport
            await client.connect(transport);
            // Fetch tools after successful connection
            let tools = [];
            try {
                const toolResult = await client.listTools();
                tools = toolResult.tools || [];
                console.log(`[${serverName}] Found ${tools.length} tools.`);
            }
            catch (toolError) {
                console.warn(`[${serverName}] Failed to list tools after connection: ${toolError instanceof Error ? toolError.message : String(toolError)}`);
                // Continue connection even if tools fail to list
            }
            // Update server state
            this.servers[serverName] = {
                client,
                transport,
                tools,
                isConnected: true,
                config: serverConfig,
            };
            return serverName; // Return server name on success
        }
        catch (error) {
            console.error(`[${serverName}] Error during connection: ${error instanceof Error ? error.message : String(error)}`);
            // Ensure server entry exists but is marked as not connected
            this.servers[serverName] = {
                // @ts-ignore
                client: undefined,
                // @ts-ignore
                transport: undefined,
                isConnected: false,
                config: serverConfig,
            };
            throw error; // Re-throw the error to be caught by connectAll
        }
    }
    /**
     * List all connected servers
     */
    getConnectedServers() {
        return Object.entries(this.servers)
            .filter(([_, connection]) => connection.isConnected)
            .map(([name]) => name);
    }
    /**
     * List all tools for a specific server
     */
    getServerTools(serverName) {
        const server = this.servers[serverName];
        if (!server || !server.isConnected) {
            throw new Error(`Server ${serverName} is not connected.`);
        }
        return server.tools || [];
    }
    /**
     * Get all tools from all connected servers.
     */
    async getAllTools() {
        const allTools = [];
        const connectedServers = this.getConnectedServers();
        for (const serverName of connectedServers) {
            try {
                // Re-fetch tools in case they changed, or use cached ones if confident
                const connection = this.servers[serverName];
                if (connection?.isConnected && connection.client) {
                    // Option 1: Use cached tools (faster, might be stale)
                    // if (connection.tools) {
                    //    allTools.push(...connection.tools);
                    // }
                    // Option 2: Re-fetch tools (slower, always up-to-date)
                    const toolResult = await connection.client.listTools();
                    const tools = toolResult.tools || [];
                    // Add server name prefix to tool name if needed for uniqueness?
                    // For now, just collect them. Ensure unique names later if necessary.
                    allTools.push(...tools);
                    // Update cache
                    connection.tools = tools;
                }
            }
            catch (error) {
                console.warn(`[${serverName}] Failed to list tools for getAllTools: ${error instanceof Error ? error.message : String(error)}`);
                // Optionally skip tools from this server if listing fails
            }
        }
        // TODO: Consider handling duplicate tool names across servers if necessary
        // For now, just return the combined list.
        return allTools;
    }
    /**
     * Execute a tool on a specific server
     */
    async executeTool(serverName, toolName, args, options = {}) {
        const startTime = Date.now();
        // Get server connection
        const server = this.servers[serverName];
        if (!server || !server.isConnected || !server.client) { // Check for client existence
            return {
                serverName,
                toolName,
                executionTime: Date.now() - startTime,
                isError: true,
                errorMessage: `Server '${serverName}' not found or not connected.`,
            };
        }
        // Find the specific tool to check schema (optional but good practice)
        const toolDefinition = server.tools?.find(t => t.name === toolName);
        if (!toolDefinition) {
            // Tool might exist but wasn't listed? Proceed cautiously or error out.
            console.warn(`[${serverName}] Tool '${toolName}' not found in listed tools. Attempting execution anyway.`);
            // return { serverName, toolName, executionTime: Date.now() - startTime, isError: true, errorMessage: `Tool '${toolName}' not found on server '${serverName}'.` };
        }
        // TODO: Validate args against toolDefinition.input_schema using Zod if desired
        // Set up timeout
        const timeoutMs = options?.timeout ? options.timeout * 1000 : this.defaultToolTimeout;
        // Show progress indicator if requested
        let progressInterval;
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
            const result = await server.client.callTool({ name: toolName, arguments: args } // Use 'arguments' field
            );
            // Check if the tool itself reported an error
            if (result.isError) {
                // Extract error message from the tool's response content if possible
                let errorMessage = `Tool '${toolName}' reported an error.`;
                // Check if result.content is an array with items 
                if (Array.isArray(result.content) && result.content.length > 0) {
                    const item = result.content[0];
                    if (item && typeof item === 'object' && 'type' in item && item.type === 'text' && 'text' in item) {
                        errorMessage = item.text;
                    }
                }
                return {
                    serverName,
                    toolName,
                    executionTime: Date.now() - startTime,
                    isError: true,
                    errorMessage: errorMessage,
                    toolResult: result.content // Include the raw error content
                };
            }
            // Return formatted result
            return {
                serverName,
                toolName,
                executionTime: Date.now() - startTime,
                toolResult: result.content, // Return the structured content
                isError: false,
            };
        }
        catch (error) {
            console.error(`[${serverName}] Error executing tool '${toolName}':`, error); // Log the error
            return {
                serverName,
                toolName,
                executionTime: Date.now() - startTime,
                isError: true,
                errorMessage: error instanceof Error ? error.message : String(error)
            };
        }
        finally {
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
    findToolProvider(toolName) {
        for (const [serverName, connection] of Object.entries(this.servers)) {
            if (!connection.isConnected || !connection.tools)
                continue;
            const hasTool = connection.tools.some(tool => tool.name === toolName);
            if (hasTool)
                return serverName;
        }
        return null;
    }
    /**
     * Close all server connections
     */
    async closeAll() {
        const closePromises = Object.entries(this.servers).map(async ([name, connection]) => {
            if (connection.isConnected) {
                console.log(`Closing connection to ${name}...`);
                try {
                    await connection.transport.close();
                    connection.isConnected = false;
                }
                catch (error) {
                    console.error(`Error closing connection to ${name}:`, error instanceof Error ? error.message : String(error));
                }
            }
        });
        await Promise.allSettled(closePromises);
        // Clean up server entries after attempting to close
        this.servers = {};
        console.log('Finished closing server connections.');
    }
}
//# sourceMappingURL=ServerManager.js.map