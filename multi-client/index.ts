import * as fs from 'node:fs';
import * as path from 'node:path';
import { fileURLToPath } from 'node:url';
import { ServerManager } from './src/ServerManager.js';
import { Repl } from './src/Repl.js';
import { ConfigFileStructure } from './src/types.js';

// Helper to get the directory name in ES modules
const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

/**
 * Main entry point for the MCP Multi-Client
 */
async function main() {
  console.log('Starting MCP Multi-Client...');
  
  // Load configuration file
  const configPath = path.join(__dirname, 'servers.json');
  let configData: ConfigFileStructure;
  
  try {
    const configFile = fs.readFileSync(configPath, 'utf-8');
    configData = JSON.parse(configFile) as ConfigFileStructure;
    
    if (!configData || typeof configData.mcpServers !== 'object') {
      throw new Error("Invalid config format: 'mcpServers' object not found.");
    }
    
    const serverNames = Object.keys(configData.mcpServers);
    console.log(`Loaded ${serverNames.length} server configurations: ${serverNames.join(', ')}`);
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === 'ENOENT') {
      // Create example servers.json if it doesn't exist
      const exampleConfig: ConfigFileStructure = {
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
    } else {
      console.error('Error loading configuration:', error instanceof Error ? error.message : String(error));
      process.exit(1);
    }
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
  } catch (error) {
    console.error('Error connecting to servers:', error instanceof Error ? error.message : String(error));
    // Continue even if some servers failed to connect
  }
  
  // Set up REPL
  const repl = new Repl(serverManager);
  
  // Set up graceful shutdown
  const shutdown = async (signal: string) => {
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
