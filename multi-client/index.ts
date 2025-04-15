import * as fs from 'node:fs';
import * as path from 'node:path';
import { fileURLToPath } from 'node:url';
import TOML from '@ltd/j-toml';
import { ServerManager } from './src/ServerManager.js';
import { Repl } from './src/Repl.js';
// Import AiConfigFileStructure and remove AiProviderConfig import from here
import type { ConfigFileStructure, AiConfigFileStructure, ProviderModelsStructure, AiProviderConfig } from './src/types.js';
import { AiClientFactory } from './src/ai/AiClientFactory.js';
import type { IAiClient } from './src/ai/IAiClient.js';
import { ConversationManager } from './src/conversation/ConversationManager.js';
import { WebServer } from './src/web/WebServer.js'; // Import WebServer

// Helper to get the directory name in ES modules
const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

/**
 * Main entry point for the MCP Multi-Client
 */
async function main() {
  console.log('Starting MCP Multi-Client...');

  // --- Configuration Loading ---
  const baseDir = path.join(__dirname);
  const serversConfigPath = path.join(baseDir, 'servers.json');
  const aiConfigPath = path.join(baseDir, 'ai_config.json'); // Path for AI config
  const providerModelsPath = path.join(baseDir, 'provider_models.toml');

  let serversConfigData: ConfigFileStructure;
  let aiConfigData: AiConfigFileStructure;
  let providerModels: ProviderModelsStructure = {};

  // --- Load Server Configuration (servers.json) ---
  try {
    const serversConfigFile = fs.readFileSync(serversConfigPath, 'utf-8');
    serversConfigData = JSON.parse(serversConfigFile) as ConfigFileStructure;

    // Basic validation
    if (!serversConfigData || typeof serversConfigData.mcpServers !== 'object') {
      throw new Error("Invalid servers.json format: 'mcpServers' object not found.");
    }

    // Ensure only 'mcpServers' key exists
    const allowedKeys = ['mcpServers'];
    const actualKeys = Object.keys(serversConfigData);
    if (actualKeys.length !== 1 || actualKeys[0] !== 'mcpServers') {
         const invalidKeys = actualKeys.filter(k => !allowedKeys.includes(k));
         throw new Error(`Invalid keys found in servers.json: ${invalidKeys.join(', ')}. Only 'mcpServers' is allowed.`);
    }

    const serverNames = Object.keys(serversConfigData.mcpServers);
    console.log(`Loaded ${serverNames.length} server configurations from servers.json: ${serverNames.join(', ')}`);
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === 'ENOENT') {
      // Create example servers.json
      const exampleServersConfig: ConfigFileStructure = {
        mcpServers: {
          example: {
            command: 'npx',
            args: ['-y', '@example/mcp-server@latest'],
            env: {}
          }
        }
        // timeouts removed
      };
      fs.writeFileSync(serversConfigPath, JSON.stringify(exampleServersConfig, null, 2), 'utf-8');
      console.error(`Server configuration file not found. An example has been created at ${serversConfigPath}.`);
      console.error('Please edit this file and restart the application.');
      process.exit(1);
    } else {
      console.error('Error loading server configuration (servers.json):', error instanceof Error ? error.message : String(error));
      process.exit(1);
    }
  }

  // --- Load AI Configuration (ai_config.json) ---
  try {
    const aiConfigFile = fs.readFileSync(aiConfigPath, 'utf-8');
    aiConfigData = JSON.parse(aiConfigFile) as AiConfigFileStructure;

    // Basic validation
    if (!aiConfigData || typeof aiConfigData.providers !== 'object') {
      throw new Error("Invalid ai_config.json format: 'providers' object not found.");
    }
    if (aiConfigData.defaultProvider && typeof aiConfigData.defaultProvider !== 'string') {
        throw new Error("Invalid ai_config.json format: 'defaultProvider' must be a string if present.");
    }
     // Ensure no other top-level keys exist
    const allowedAiKeys = ['defaultProvider', 'providers'];
    for (const key in aiConfigData) {
        if (!allowedAiKeys.includes(key)) {
            console.warn(`Warning: Unexpected key "${key}" found in ai_config.json. It will be ignored.`);
        }
    }

    console.log(`Loaded AI configuration from ai_config.json for providers: ${Object.keys(aiConfigData.providers).join(', ')}`);
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === 'ENOENT') {
      // Create example ai_config.json
      const exampleAiConfig: AiConfigFileStructure = {
        defaultProvider: "anthropic", // Example default
        providers: {
          anthropic: {
            provider: "anthropic",
            model: "claude-3-5-sonnet-20240620",
            apiKeyEnvVar: "ANTHROPIC_API_KEY",
            temperature: 0.7
          },
          openai: {
            provider: "openai",
            model: "gpt-4o-mini",
            apiKeyEnvVar: "OPENAI_API_KEY"
            // temperature defaults if omitted
          }
        }
      };
      fs.writeFileSync(aiConfigPath, JSON.stringify(exampleAiConfig, null, 2), 'utf-8');
      console.error(`AI configuration file not found. An example has been created at ${aiConfigPath}.`);
      console.error('Please edit this file (especially API keys/models) and restart.');
      process.exit(1);
    } else {
      console.error('Error loading AI configuration (ai_config.json):', error instanceof Error ? error.message : String(error));
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
              if (typeof value === 'object' && value !== null && Array.isArray((value as any).models)) {
                  // Ensure models in the array are strings
                  const modelsArray = (value as any).models;
                  if (modelsArray.every((m: unknown) => typeof m === 'string')) {
                     acc[key.toLowerCase()] = { models: modelsArray as string[] };
                  } else {
                     console.warn(`Invalid model list for provider "${key}" in ${providerModelsPath}. Contains non-string elements. Skipping.`);
                  }
              } else {
                  console.warn(`Invalid structure for provider "${key}" in ${providerModelsPath}. Expected object with 'models' array. Skipping.`);
              }
              return acc;
          }, {} as ProviderModelsStructure);
          console.log(`Loaded model suggestions from ${providerModelsPath} for providers: ${Object.keys(providerModels).join(', ')}`);
      } else {
          console.warn(`Could not parse ${providerModelsPath} into a valid object.`);
      }

    } else {
      console.warn(`Provider models file not found at ${providerModelsPath}. Model suggestions will not be used.`);
    }
  } catch (error) {
    console.error(`Error loading or parsing ${providerModelsPath}:`, error instanceof Error ? error.message : String(error));
    // Continue without model suggestions
  }


  // --- AI Client Initialization ---
  let aiClient: IAiClient | null = null;
  let conversationManager: ConversationManager | null = null;
  const providerNames = Object.keys(aiConfigData.providers || {}); // Use aiConfigData

  const defaultProviderName = aiConfigData.defaultProvider; // Use aiConfigData
  const aiProviders = aiConfigData.providers; // Use aiConfigData

  if (defaultProviderName && aiProviders && aiProviders[defaultProviderName]) {
      try {
          const defaultProviderConfig: AiProviderConfig = aiProviders[defaultProviderName]; // Explicit type
          // Pass the loaded model suggestions to the factory
          aiClient = AiClientFactory.createClient(defaultProviderConfig, providerModels);
          console.log(`Initialized default AI client: ${defaultProviderName} (${aiClient.getModelName()})`);
      } catch (error) {
          console.error(`Failed to initialize default AI provider "${defaultProviderName}" from ai_config.json:`, error instanceof Error ? error.message : String(error));
          console.error("Chat functionality will be disabled. Check your AI configuration and API keys.");
          // Continue without AI client
      }
  } else if (providerNames.length > 0) {
       console.warn("No default AI provider specified or the specified default is invalid. Chat will be disabled until a provider is selected (feature not yet implemented).");
  } else {
      console.warn("No AI providers configured. Chat functionality is disabled.");
  }


  // --- Server Manager Initialization ---
  const serverManager = new ServerManager(serversConfigData); // Use serversConfigData

  // --- Conversation Manager Initialization (if AI client is available) ---
  if (aiClient) {
      conversationManager = new ConversationManager(aiClient, serverManager);
  } else {
      // Create a dummy or null ConversationManager if needed by Repl, or handle in Repl
      console.log("ConversationManager not created due to missing AI client.");
      // conversationManager = new DummyConversationManager(); // Or handle null in Repl
  }


  // --- Connect to Servers ---
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

  // --- REPL Setup ---
  // Handle the case where conversationManager might be null
  if (!conversationManager) {
      console.error("Cannot start REPL in chat mode without a configured AI provider. Exiting.");
      // Optionally, start REPL in a limited command-only mode
      // For now, let's exit if the primary purpose (chat) isn't available.
      await serverManager.closeAll(); // Clean up connected servers
      process.exit(1);
  }

  const repl = new Repl(serverManager, conversationManager); // Pass conversationManager

  // --- Initialize Web Server (if enabled in command line args) ---
  let webServer: WebServer | null = null;
  const useWeb = process.argv.includes('--web') || process.argv.includes('-w');
  const webPort = 3000; // Default port for web server
  
  if (useWeb && conversationManager) {
    webServer = new WebServer(conversationManager, serverManager, webPort);
    webServer.start();
    console.log(`Web interface available at http://localhost:${webPort}`);
  }

  // --- Graceful Shutdown ---
  const shutdown = async (signal: string) => {
    console.log(`\nReceived ${signal}. Shutting down...`);
    
    // Stop REPL
    repl.stop();
    
    // Stop web server if running
    if (webServer) {
      await webServer.stop();
    }
    
    // Close server connections
    await serverManager.closeAll();
    
    console.log('All server connections closed.');
    process.exit(0);
  };
  
  // Handle termination signals
  process.on('SIGINT', () => shutdown('SIGINT'));
  process.on('SIGTERM', () => shutdown('SIGTERM'));
  
  // Start REPL if web interface is not enabled or if explicitly requested
  const useRepl = !useWeb || process.argv.includes('--repl') || process.argv.includes('-r');
  if (useRepl) {
    repl.start();
    if (useWeb) {
      console.log('Running in both REPL and web mode. Press Ctrl+C in this terminal to stop both.');
    }
  } else if (useWeb) {
    console.log('Running in web-only mode. Press Ctrl+C to stop.');
  }
}

// Run the main function
main().catch(error => {
  console.error('Unhandled error in main function:', error instanceof Error ? error.message : String(error));
  process.exit(1);
});
