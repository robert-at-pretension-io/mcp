import * as fs from 'node:fs';
import * as path from 'node:path';
import { fileURLToPath } from 'node:url';
import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { StdioClientTransport } from '@modelcontextprotocol/sdk/client/stdio.js';
// Helper to get the directory name in ES modules
const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
async function main() {
    const configPath = path.join(__dirname, 'servers.json');
    let configData;
    // Read and parse the configuration file
    try {
        const configFile = fs.readFileSync(configPath, 'utf-8');
        configData = JSON.parse(configFile);
        if (!configData || typeof configData.mcpServers !== 'object') {
            throw new Error("Invalid config format: 'mcpServers' object not found.");
        }
        const serverNames = Object.keys(configData.mcpServers);
        console.log(`Loaded ${serverNames.length} server configurations from ${configPath}: ${serverNames.join(', ')}`);
    }
    catch (error) {
        console.error(`Error reading or parsing configuration file ${configPath}:`, error instanceof Error ? error.message : error);
        process.exit(1);
    }
    // Array to hold connection promises
    const connectionPromises = [];
    // Store clients and their transports for graceful shutdown
    const serverConnections = {};
    // Iterate over configurations and initiate connections
    for (const [serverName, config] of Object.entries(configData.mcpServers)) {
        console.log(`\nAttempting to connect to server: ${serverName}`);
        let transport;
        try {
            // All servers are assumed stdio type based on the new config structure
            const transportOptions = {
                command: config.command,
                args: config.args,
                // NOTE: The SDK's StdioClientTransport might not directly support 'env'.
                // This property is included for potential future SDK updates.
                // The environment variables from the config are NOT currently passed to the child process.
                env: config.env,
            };
            transport = new StdioClientTransport(transportOptions);
            console.log(`  -> Using stdio transport: ${config.command} ${config.args?.join(' ') ?? ''}`);
            if (config.env) {
                console.log(`  -> Configured environment variables (NOTE: currently not passed by SDK): ${Object.keys(config.env).join(', ')}`);
            }
            // Create the MCP client instance
            const clientInfo = {
                name: `multi-client-${serverName.replace(/\s+/g, '-')}`, // Use server name from config key
                version: '1.0.0',
            };
            const client = new Client(clientInfo);
            // Store client and transport together
            serverConnections[serverName] = { client, transport };
            // Add error handling for the transport
            transport.onerror = (error) => {
                console.error(`[${serverName}] Transport error:`, error.message);
            };
            transport.onclose = () => {
                console.log(`[${serverName}] Connection closed.`);
            };
            // Initiate connection and add the promise to the array
            const connectPromise = client.connect(transport)
                .then(async () => {
                console.log(`[${serverName}] Successfully connected!`);
                // Example: List tools after connection
                try {
                    const res = await client.listTools();
                    console.log(`[${serverName}] Tools:`, res.tools.map(t => t.name));
                }
                catch (err) {
                    console.error(`[${serverName}] Error listing tools:`, err instanceof Error ? err.message : err);
                }
            })
                .catch(error => {
                console.error(`[${serverName}] Failed to connect:`, error instanceof Error ? error.message : error);
            });
            connectionPromises.push(connectPromise);
        }
        catch (error) {
            console.error(`[${serverName}] Error setting up client:`, error instanceof Error ? error.message : error);
        }
    }
    // Wait for all initial connection attempts to settle
    await Promise.allSettled(connectionPromises);
    console.log("\nAll server connection attempts finished.");
    console.log("\nAll server connection attempts finished.");
    console.log("Client is running. Press Ctrl+C to exit gracefully.");
    // Setup graceful shutdown
    setupShutdownHandler(serverConnections);
    // No need for setInterval anymore, the process will stay alive
    // due to open connections or wait for signals.
}
/**
 * Sets up signal handlers for graceful shutdown.
 * @param connections Record mapping server names to their client and transport.
 */
function setupShutdownHandler(connections) {
    const shutdown = async (signal) => {
        console.log(`\nReceived ${signal}. Shutting down servers...`);
        const closePromises = Object.entries(connections).map(([name, { transport }]) => {
            console.log(`  Closing connection to ${name}...`);
            return transport.close().catch(err => {
                console.error(`  Error closing transport for ${name}:`, err instanceof Error ? err.message : err);
            });
        });
        await Promise.allSettled(closePromises);
        console.log("All server connections closed.");
        process.exit(0); // Exit cleanly
    };
    process.on('SIGINT', () => shutdown('SIGINT')); // Ctrl+C
    process.on('SIGTERM', () => shutdown('SIGTERM')); // Termination signal
}
main().catch(error => {
    console.error("Unhandled error in main function:", error);
    process.exit(1);
});
//# sourceMappingURL=index.js.map