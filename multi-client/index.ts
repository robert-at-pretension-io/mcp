import * as fs from 'node:fs';
import * as path from 'node:path';
import { fileURLToPath } from 'node:url';
import TOML from '@ltd/j-toml';
import { ServerManager } from './src/ServerManager.js';
import { Repl } from './src/Repl.js';
import * as readline from 'node:readline'; // Import readline for prompting
// Import AiConfigFileStructure and remove AiProviderConfig import from here
import type { ConfigFileStructure, AiConfigFileStructure, ProviderModelsStructure, AiProviderConfig } from './src/types.js';
import { AiClientFactory, MissingApiKeyError } from './src/ai/AiClientFactory.js'; // Import Factory and Error
import type { IAiClient } from './src/ai/IAiClient.js';
import { ConversationManager } from './src/conversation/ConversationManager.js';
import { WebServer } from './src/web/WebServer.js';

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
  // Use the new function that handles prompting for keys
  const aiClient = await initializeAiClientWithPrompting(aiConfigData, providerModels, aiConfigPath);
  let conversationManager: ConversationManager | null = null;

  // --- Server Manager Initialization ---
  const serverManager = new ServerManager(serversConfigData); // Use serversConfigData

  // --- Conversation Manager Initialization (if AI client is available) ---
  if (aiClient) {
      conversationManager = new ConversationManager(aiClient, serverManager, providerModels);
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

  const repl = new Repl(serverManager, conversationManager, providerModels); // Pass conversationManager and providerModels

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

// --- Helper Functions ---

/**
 * Prompts the user for input, optionally hiding the input (for passwords/keys).
 */
async function promptForInput(promptText: string, hideInput: boolean = false): Promise<string> {
  const rl = readline.createInterface({
    input: process.stdin,
    output: process.stdout,
    terminal: true // Ensure terminal features are enabled
  });

  // Hacky way to hide input in standard readline
  // Use process.stdout directly as it's the stream being used by the interface
  const outputStream = process.stdout as NodeJS.WritableStream;
  const originalWrite = outputStream.write;

  if (hideInput) {
    // Override the write method with the correct signature
    outputStream.write = (chunk: any, encodingOrCb?: BufferEncoding | ((err?: Error | null | undefined) => void), cb?: (err?: Error | null | undefined) => void): boolean => {
      let encoding: BufferEncoding | undefined;
      let callback: ((err?: Error | null | undefined) => void) | undefined;

      if (typeof encodingOrCb === 'function') {
        callback = encodingOrCb;
        encoding = undefined;
      } else {
        encoding = encodingOrCb;
        callback = cb;
      }

      if (typeof chunk === 'string') {
        switch (chunk) {
          case '\r\n':
          case '\n':
            // Keep newlines - call original write
            return originalWrite.call(outputStream, chunk, encoding, callback);
          default:
            // Replace other characters with '*'
            const starChunk = '*'.repeat(chunk.length); // Ensure same length for cursor positioning
            return originalWrite.call(outputStream, starChunk, encoding, callback);
        }
      }
      // Fallback for non-string chunks or if write returns false
      return originalWrite.call(outputStream, chunk, encoding, callback);
    };
  }

  return new Promise((resolve) => {
    rl.question(promptText, (answer) => {
      if (hideInput) {
        outputStream.write = originalWrite; // Restore original write method
        process.stdout.write('\n'); // Add newline after hidden input
      }
      rl.close();
      resolve(answer.trim());
    });
  });
}


/**
 * Initializes the AI client, prompting for missing API keys if necessary.
 */
async function initializeAiClientWithPrompting(
  aiConfigData: AiConfigFileStructure,
  providerModels: ProviderModelsStructure,
  aiConfigPath: string
): Promise<IAiClient | null> {
  const providerNames = Object.keys(aiConfigData.providers || {});
  const defaultProviderName = aiConfigData.defaultProvider;
  const aiProviders = aiConfigData.providers;

  if (!defaultProviderName || !aiProviders || !aiProviders[defaultProviderName]) {
    if (providerNames.length > 0) {
      console.warn("No default AI provider specified or the specified default is invalid in ai_config.json. Chat will be disabled.");
    } else {
      console.warn("No AI providers configured in ai_config.json. Chat functionality is disabled.");
    }
    return null;
  }

  let defaultProviderConfig: AiProviderConfig = { ...aiProviders[defaultProviderName] }; // Clone to allow modification
  let aiClient: IAiClient | null = null;
  let retries = 3; // Limit retries for prompting

  while (retries > 0 && aiClient === null) {
    try {
      aiClient = AiClientFactory.createClient(defaultProviderConfig, providerModels);
      console.log(`Initialized default AI client: ${defaultProviderName} (${aiClient.getModelName()})`);
    } catch (error) {
      if (error instanceof MissingApiKeyError) {
        console.warn(`Configuration requires environment variable "${error.apiKeyEnvVar}" for provider "${error.providerName}", but it's not set.`);
        const apiKey = await promptForInput(`Enter API Key for ${error.providerName} (or press Enter to skip): `, true);

        if (!apiKey) {
          console.error(`API Key not provided for ${error.providerName}. Cannot initialize this provider.`);
          return null; // Stop trying for this provider
        }

        // Key provided: Update environment for this session
        process.env[error.apiKeyEnvVar] = apiKey;
        console.log(`API Key for ${error.providerName} set for this session.`);

        // Update the config file
        try {
          // Read the current config file again to avoid overwriting concurrent changes (less likely here, but good practice)
          const currentAiConfigFile = fs.readFileSync(aiConfigPath, 'utf-8');
          const currentAiConfigData = JSON.parse(currentAiConfigFile) as AiConfigFileStructure;

          // Find the provider and update it
          if (currentAiConfigData.providers && currentAiConfigData.providers[error.providerName]) {
            console.log(`Saving API key directly to ${aiConfigPath} for provider "${error.providerName}".`);
            console.warn("SECURITY WARNING: Storing API keys directly in configuration files is not recommended.");

            // Add apiKey, remove apiKeyEnvVar
            currentAiConfigData.providers[error.providerName].apiKey = apiKey;
            delete currentAiConfigData.providers[error.providerName].apiKeyEnvVar;

            // Write back to the file
            fs.writeFileSync(aiConfigPath, JSON.stringify(currentAiConfigData, null, 2), 'utf-8');
            console.log(`Configuration updated in ${aiConfigPath}.`);

            // Update the in-memory config for the retry
            defaultProviderConfig = { ...currentAiConfigData.providers[error.providerName] };

          } else {
            console.error(`Could not find provider "${error.providerName}" in ${aiConfigPath} to save the key.`);
            // Continue retry with the key set in process.env for this session
          }
        } catch (writeError) {
          console.error(`Error writing updated configuration to ${aiConfigPath}:`, writeError);
          // Continue retry with the key set in process.env for this session
        }

        // Decrement retries and loop to try creating the client again
        retries--;

      } else {
        // Different error, rethrow
        console.error(`Failed to initialize default AI provider "${defaultProviderName}" from ai_config.json:`, error instanceof Error ? error.message : String(error));
        console.error("Chat functionality will be disabled. Check your AI configuration.");
        return null; // Stop trying
      }
    }
  } // End while loop

  if (!aiClient) {
     console.error(`Failed to initialize AI client for ${defaultProviderName} after multiple attempts.`);
  }

  return aiClient;
}
