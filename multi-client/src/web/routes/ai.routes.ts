import { Router } from 'express'; // Remove Request, Response, NextFunction imports if unused elsewhere
import type { ConversationManager } from '../../conversation/ConversationManager.js';
import type { ServerManager } from '../../ServerManager.js';
import * as fs from 'node:fs/promises';
import * as fsSync from 'node:fs'; // For existsSync
import * as path from 'node:path';
import { fileURLToPath } from 'node:url';
import type { AiConfigFileStructure, ProviderModelsStructure } from '../../types.js';

// Helper for paths
const __filename = fileURLToPath(import.meta.url);
// Correct baseDir calculation: up 3 levels from dist/src/web/routes/
const routesDir = path.dirname(__filename);
const baseDir = path.join(routesDir, '../../..'); // Project root

export function createAiRouter(
    conversationManager: ConversationManager,
    serverManager: ServerManager // Inject ServerManager if needed by routes here
): Router {
    const router = Router();
    const aiConfigPath = path.join(baseDir, 'ai_config.json');
    const providerModelsPath = path.join(baseDir, 'provider_models.toml');

    // Helper function to read AI config within handlers
    async function readAiConfig(): Promise<AiConfigFileStructure> {
        try {
            const aiConfigFile = await fs.readFile(aiConfigPath, 'utf-8');
            return JSON.parse(aiConfigFile) as AiConfigFileStructure;
        } catch (error) {
            console.error(`Error reading AI config file (${aiConfigPath}):`, error);
            throw new Error(`Failed to read AI configuration: ${error instanceof Error ? error.message : 'Unknown error'}`);
        }
    }

    // Helper function to read Provider Models within handlers
    async function readProviderModels(): Promise<ProviderModelsStructure> {
        let providerModels: ProviderModelsStructure = {};
        try {
            // Use fsSync.existsSync for a quick check before async read
            if (fsSync.existsSync(providerModelsPath)) {
                const providerModelsFile = await fs.readFile(providerModelsPath, 'utf-8');
                const TOML = (await import('@ltd/j-toml')).default; // Dynamic import inside async function
                const parsedToml = TOML.parse(providerModelsFile, { joiner: '\n', bigint: false });

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
                }
            }
        } catch (tomlError) {
            console.error(`Error loading or parsing ${providerModelsPath}:`, tomlError);
            // Continue without models if parsing fails, return empty object
        }
        return providerModels;
    }


    // --- AI Model Info ---
    router.get('/model', (req: Request, res: Response) => { // Add types
        try {
            const model = conversationManager.getAiClientModelName();
            const provider = conversationManager.getAiProviderName(); // Use the new getter
            res.json({ model, provider }); // Return both
        } catch (error) {
            console.error("Error in /api/ai/model:", error); // Log the specific error
            res.status(500).json({ error: `Failed to get AI model info: ${error instanceof Error ? error.message : String(error)}` });
        }
    });

    // --- AI Providers Info ---
    router.get('/providers', async (req, res) => { // Remove explicit types
        try {
            // Load config and models inside the handler
            const aiConfigData = await readAiConfig();
            const providerModels = await readProviderModels();

            res.json({
                current: aiConfigData.defaultProvider || '', // Ensure current is always a string
                providers: aiConfigData.providers,
                models: providerModels
            });
        } catch (error) {
            res.status(500).json({ error: `Failed to get AI providers info: ${error instanceof Error ? error.message : String(error)}` });
        }
    });

    // --- Switch AI Provider ---
    // Note: This endpoint might be deprecated in favor of /model endpoint handling provider change
    router.post('/provider', async (req, res) => { // Remove explicit types
        try {
            const { provider } = req.body;
            if (!provider) {
                return res.status(400).json({ error: 'Provider name is required' });
            }

            // Load config and models inside the handler
            const aiConfigData = await readAiConfig();
            const providerModels = await readProviderModels();

            if (!aiConfigData.providers[provider]) {
                return res.status(404).json({ error: `Provider "${provider}" not found` });
            }

            // Update default provider in config data
            aiConfigData.defaultProvider = provider;
            await fs.writeFile(aiConfigPath, JSON.stringify(aiConfigData, null, 2), 'utf-8');

            // Switch the client in ConversationManager
            const providerConfig = aiConfigData.providers[provider];
            const model = conversationManager.switchAiClient(providerConfig, providerModels);

            // Emit model changed event via ConversationManager or directly if needed
            // Assuming ConversationManager handles emitting events or WebServer does based on return

            res.json({ provider, model });
        } catch (error) {
            res.status(500).json({ error: `Failed to switch provider: ${error instanceof Error ? error.message : String(error)}` });
        }
    });

    // --- Switch AI Model (can also handle provider change) ---
    router.post('/model', async (req, res) => { // Remove explicit types
        try {
            const { model, provider } = req.body; // Provider is optional, defaults to current
            if (!model) {
                return res.status(400).json({ error: 'Model name is required' });
            }

            // Load config and models inside the handler
            const aiConfigData = await readAiConfig();
            const providerModels = await readProviderModels();

            const providerName = provider || aiConfigData.defaultProvider;
            if (!providerName || !aiConfigData.providers[providerName]) {
                return res.status(404).json({ error: `Provider "${providerName}" not found` });
            }

            // Update the provider's model in config data
            aiConfigData.providers[providerName].model = model;
            // If provider was explicitly passed and is different, update defaultProvider too
            if (provider && provider !== aiConfigData.defaultProvider) {
                aiConfigData.defaultProvider = provider;
            }
            await fs.writeFile(aiConfigPath, JSON.stringify(aiConfigData, null, 2), 'utf-8');

            // Switch the client in ConversationManager
            const providerConfig = aiConfigData.providers[providerName];
            const actualModel = conversationManager.switchAiClient(providerConfig, providerModels);

            // Emit model changed event via ConversationManager or directly

            res.json({ provider: providerName, model: actualModel });
        } catch (error) {
            res.status(500).json({ error: `Failed to switch model: ${error instanceof Error ? error.message : String(error)}` });
        }
    });

    // --- Update API Keys ---
    router.post('/keys', async (req, res) => { // Remove explicit types
        try {
            const { provider, apiKey } = req.body;
            if (!provider || !apiKey) {
                return res.status(400).json({ error: 'Provider and API key are required' });
            }

            // Load config inside the handler
            const aiConfigData = await readAiConfig();

            if (!aiConfigData.providers[provider]) {
                return res.status(404).json({ error: `Provider "${provider}" not found` });
            }

            // Update the API key, remove env var if present
            aiConfigData.providers[provider].apiKey = apiKey;
            delete aiConfigData.providers[provider].apiKeyEnvVar;

            // Save the updated config
            await fs.writeFile(aiConfigPath, JSON.stringify(aiConfigData, null, 2), 'utf-8');

            // Reload the AI client if this is the current provider
            if (provider === aiConfigData.defaultProvider) {
                try {
                    // Load provider models inside the handler
                    const providerModels = await readProviderModels();

                    const providerConfig = aiConfigData.providers[provider];
                    const model = conversationManager.switchAiClient(providerConfig, providerModels);

                    // Emit model changed event (key changed)
                    // This should ideally be handled by the caller (WebServer) after getting the response

                    res.json({
                        success: true,
                        message: 'API key updated and applied.',
                        provider,
                        model
                    });
                } catch (clientError) {
                    console.error('Error switching client after API key update:', clientError);
                    res.json({
                        success: true,
                        warning: 'API key was saved but could not be applied immediately.',
                        error: String(clientError)
                    });
                }
            } else {
                res.json({
                    success: true,
                    message: `API key updated for provider: ${provider}`
                });
            }
        } catch (error) {
            res.status(500).json({ error: `Failed to update API key: ${error instanceof Error ? error.message : String(error)}` });
        }
    });


    return router;
}
