import * as fs from 'node:fs';
import * as path from 'node:path';
import { fileURLToPath } from 'node:url';
import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { StdioClientTransport } from '@modelcontextprotocol/sdk/client/stdio.js';
import { SSEClientTransport } from '@modelcontextprotocol/sdk/client/sse.js';
import type { Transport } from '@modelcontextprotocol/sdk/shared/transport.js';
import type { StdioServerParameters } from '@modelcontextprotocol/sdk/client/stdio.js';

// Define types for the configuration structure

// Configuration for a single stdio server
interface StdioServerConfig extends Omit<StdioServerParameters, 'env'> { // Omit env if StdioServerParameters includes it, otherwise just extend
    env?: Record<string, string>;
}

// Structure of the servers.json file
interface ConfigFileStructure {
    mcpServers: Record<string, StdioServerConfig>;
}


// Helper to get the directory name in ES modules
const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

async function main() {
    const configPath = path.join(__dirname, 'servers.json');
    let configData: ConfigFileStructure;

    // Read and parse the configuration file
    try {
        const configFile = fs.readFileSync(configPath, 'utf-8');
        configData = JSON.parse(configFile) as ConfigFileStructure;
        if (!configData || typeof configData.mcpServers !== 'object') {
            throw new Error("Invalid config format: 'mcpServers' object not found.");
        }
        const serverNames = Object.keys(configData.mcpServers);
        console.log(`Loaded ${serverNames.length} server configurations from ${configPath}: ${serverNames.join(', ')}`);
    } catch (error) {
        console.error(`Error reading or parsing configuration file ${configPath}:`, error instanceof Error ? error.message : error);
        process.exit(1);
    }

    // Array to hold connection promises
    const connectionPromises: Promise<void>[] = [];
    const clients: Record<string, Client> = {}; // Store clients by name

    // Iterate over configurations and initiate connections
    for (const [serverName, config] of Object.entries(configData.mcpServers)) {
        console.log(`\nAttempting to connect to server: ${serverName}`);

        let transport: Transport;
        try {
            // All servers are assumed stdio type based on the new config structure
            const transportOptions: StdioServerParameters & { env?: Record<string, string> } = {
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
            const client = new Client({
                name: `multi-client-${serverName.replace(/\s+/g, '-')}`, // Use server name from config key
                version: '1.0.0',
            });
            clients[serverName] = client; // Store the client

            // Add error handling for the transport
            transport.onerror = (error) => {
                console.error(`[${serverName}] Transport error:`, error.message);
            };
            transport.onclose = () => {
                 console.log(`[${serverName}] Connection closed.`);
            };


            // Initiate connection and add the promise to the array
            const connectPromise = client.connect(transport)
                .then(() => {
                    console.log(`[${serverName}] Successfully connected!`);
                    // Example: List tools after connection
                    client.listTools().then(res => console.log(`[${serverName}] Tools:`, res.tools.map(t => t.name))).catch(err => console.error(`[${serverName}] Error listing tools:`, err.message));
                })
                .catch(error => {
                    console.error(`[${serverName}] Failed to connect:`, error instanceof Error ? error.message : error);
                });

            connectionPromises.push(connectPromise);

        } catch (error) {
             console.error(`[${serverName}] Error setting up client:`, error instanceof Error ? error.message : error);
        }
    }

    // Wait for all initial connection attempts to settle
    await Promise.allSettled(connectionPromises);

    console.log("\nAll server connection attempts finished.");
    console.log("Client is running. Press Ctrl+C to exit.");

    // Keep the process running to maintain connections
    // You might want more sophisticated logic here depending on the application's needs
    setInterval(() => {}, 1 << 30); // Keep Node.js event loop alive
}

main().catch(error => {
    console.error("Unhandled error in main function:", error);
    process.exit(1);
});
