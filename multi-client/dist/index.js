import * as fs from 'node:fs';
import * as fs from 'node:fs';
import * as path from 'node:path';
import { fileURLToPath } from 'node:url';
import TOML from '@ltd/j-toml'; // Import TOML parser
import { ServerManager } from './src/ServerManager.js';
import { Repl } from './src/Repl.js';
// Helper to get the directory name in ES modules
const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
/**
 * Main entry point for the MCP Multi-Client
 */
async function main() {
    console.log('Starting MCP Multi-Client...');
    // --- Configuration Loading ---
    // Determine base directory (assuming index.js is in dist/)
    const baseDir = path.join(__dirname); // Adjust if index.js is elsewhere relative to config files
    const configPath = path.join(baseDir, 'servers.json');
    const providerModelsPath = path.join(baseDir, 'provider_models.toml'); // Path to the TOML file
    let configData;
    let providerModels = {}; // Initialize empty model suggestions
    try {
        // Read main config file (servers.json)
        const configFile = fs.readFileSync(configPath, 'utf-8');
        configData = JSON.parse(configFile);
        if (!configData || typeof configData.mcpServers !== 'object') {
            throw new Error("Invalid config format: 'mcpServers' object not found.");
        }
        const serverNames = Object.keys(configData.mcpServers);
        console.log(`Loaded ${serverNames.length} server configurations: ${serverNames.join(', ')}`);
    }
    catch (error) {
        if (error.code === 'ENOENT') {
            // Create example servers.json if it doesn't exist
            const exampleConfig = {
                mcpServers: {
                    example: {
                        command: 'npx',
                        args: ['-y', '@example/mcp-server@latest'],
                        env: {}
                    }
                },
                timeouts: {
                    request: 120,
                    tool: 300
                }
            };
            fs.writeFileSync(configPath, JSON.stringify(exampleConfig, null, 2), 'utf-8');
            console.error(`Configuration file not found. An example has been created at ${configPath}.`);
            console.error('Please edit this file and restart the application.');
            process.exit(1);
        }
        else {
            console.error('Error loading configuration:', error instanceof Error ? error.message : String(error));
            process.exit(1);
        }
    }
    // --- Load Provider Model Suggestions (provider_models.toml) ---
    try {
        if (fs.existsSync(providerModelsPath)) {
            const providerModelsFile = fs.readFileSync(providerModelsPath, 'utf-8');
            // Use TOML.parse, ensuring it handles the structure correctly
            // The library might return a Table object, convert if necessary
            const parsedToml = TOML.parse(providerModelsFile, { joiner: '\n', bigint: false });
            // Assuming the TOML structure is { provider: { models: [...] } }
            // We need to ensure the parsed structure matches ProviderModelsStructure
            if (typeof parsedToml === 'object' && parsedToml !== null) {
                providerModels = Object.entries(parsedToml).reduce((acc, [key, value]) => {
                    // Ensure value is an object and has a 'models' array property
                    if (typeof value === 'object' && value !== null && Array.isArray(value.models)) {
                        // Ensure models in the array are strings
                        const modelsArray = value.models;
                        if (modelsArray.every((m) => typeof m === 'string')) {
                            acc[key.toLowerCase()] = { models: modelsArray };
                        }
                        else {
                            console.warn(`Invalid model list for provider "${key}" in ${providerModelsPath}. Contains non-string elements. Skipping.`);
                        }
                    }
                    else {
                        console.warn(`Invalid structure for provider "${key}" in ${providerModelsPath}. Expected object with 'models' array. Skipping.`);
                    }
                    return acc;
                }, {});
                console.log(`Loaded model suggestions from ${providerModelsPath} for providers: ${Object.keys(providerModels).join(', ')}`);
            }
            else {
                console.warn(`Could not parse ${providerModelsPath} into a valid object.`);
            }
        }
        else {
            console.warn(`Provider models file not found at ${providerModelsPath}. Model suggestions will not be used.`);
        }
    }
    catch (error) {
        console.error(`Error loading or parsing ${providerModelsPath}:`, error instanceof Error ? error.message : String(error));
        // Continue without model suggestions
    }
    // Create server manager
    const serverManager = new ServerManager(configData);
    // Connect to all servers
    try {
        console.log('Connecting to configured servers...');
        const connectedServers = await serverManager.connectAll();
        console.log(`Successfully connected to ${connectedServers.length} servers.`);
        if (connectedServers.length === 0) {
            console.warn('Warning: No servers connected. Check your configuration and server status.');
        }
    }
    catch (error) {
        console.error('Error connecting to servers:', error instanceof Error ? error.message : String(error));
        // Continue even if some servers failed to connect
    }
    // Set up REPL
    const repl = new Repl(serverManager);
    // Set up graceful shutdown
    const shutdown = async (signal) => {
        console.log(`\nReceived ${signal}. Shutting down...`);
        // Stop REPL
        repl.stop();
        // Close server connections
        await serverManager.closeAll();
        console.log('All server connections closed.');
        process.exit(0);
    };
    // Handle termination signals
    process.on('SIGINT', () => shutdown('SIGINT'));
    process.on('SIGTERM', () => shutdown('SIGTERM'));
    // Start REPL
    repl.start();
}
// Run the main function
main().catch(error => {
    console.error('Unhandled error in main function:', error instanceof Error ? error.message : String(error));
    process.exit(1);
});
//# sourceMappingURL=index.js.map
