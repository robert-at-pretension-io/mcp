import { Router, type Request, type Response, type RequestHandler } from 'express'; // Add Request, Response, RequestHandler types
import * as fs from 'node:fs/promises';
import * as fsSync from 'node:fs';
import * as path from 'node:path';
import { fileURLToPath } from 'node:url';

// Helper for paths
const __filename = fileURLToPath(import.meta.url);
// Correct baseDir calculation: up 3 levels from dist/src/web/routes/
const routesDir = path.dirname(__filename);
const baseDir = path.join(routesDir, '../../..'); // Project root

export function createConfigRouter(): Router {
    const router = Router();
    const allowedFiles = ['ai_config.json', 'servers.json', 'provider_models.toml'];

    // --- Get Config File Content ---
    router.get('/:file', (async (req, res) => { // Remove explicit types, add RequestHandler cast
        try {
            const { file } = req.params;

            if (!allowedFiles.includes(file)) {
                return res.status(403).json({ error: `Access to file "${file}" is not allowed` });
            }

            const filePath = path.join(baseDir, file);

            try {
                await fs.access(filePath, fs.constants.F_OK); // Check if file exists asynchronously
            } catch (accessError) {
                 return res.status(404).json({ error: `File "${file}" not found` });
            }

            const content = await fs.readFile(filePath, 'utf-8');

            res.json({
                file,
                path: filePath, // Consider if exposing the full path is okay
                content,
                type: file.endsWith('.json') ? 'json' : file.endsWith('.toml') ? 'toml' : 'text'
            });
        } catch (error) {
            res.status(500).json({ error: `Failed to read configuration file: ${error instanceof Error ? error.message : String(error)}` });
        }
    }) as RequestHandler);

    // --- Save Config File Content ---
    router.post('/:file', (async (req, res) => { // Remove explicit types, add RequestHandler cast
        try {
            const { file } = req.params;
            const { content } = req.body;

            if (content === undefined) {
                return res.status(400).json({ error: 'File content is required' });
            }
            if (typeof content !== 'string') {
                 return res.status(400).json({ error: 'File content must be a string' });
            }


            if (!allowedFiles.includes(file)) {
                return res.status(403).json({ error: `Access to file "${file}" is not allowed` });
            }

            const filePath = path.join(baseDir, file);

            // Validate content based on file type and structure
            try {
                if (file === 'servers.json') {
                    const parsedConfig = JSON.parse(content) as ConfigFileStructure;
                    if (typeof parsedConfig !== 'object' || parsedConfig === null || typeof parsedConfig.mcpServers !== 'object' || parsedConfig.mcpServers === null) {
                        throw new Error("Invalid servers.json structure: Must be an object with an 'mcpServers' object.");
                    }
                    // Further validation of StdioServerConfig structure within mcpServers
                    for (const [name, cfg] of Object.entries(parsedConfig.mcpServers)) {
                         if (!cfg || typeof cfg !== 'object' || typeof cfg.command !== 'string' || !cfg.command) {
                             throw new Error(`Invalid config for server "${name}": Missing or invalid 'command'.`);
                         }
                         if (cfg.args && !Array.isArray(cfg.args)) throw new Error(`Invalid config for server "${name}": 'args' must be an array.`);
                         if (cfg.env && typeof cfg.env !== 'object') throw new Error(`Invalid config for server "${name}": 'env' must be an object.`);
                         // Check for unexpected keys in StdioServerConfig if needed
                    }
                } else if (file === 'ai_config.json') {
                    const parsedConfig = JSON.parse(content) as AiConfigFileStructure;
                     if (typeof parsedConfig !== 'object' || parsedConfig === null || typeof parsedConfig.providers !== 'object' || parsedConfig.providers === null) {
                        throw new Error("Invalid ai_config.json structure: Must be an object with a 'providers' object.");
                    }
                     if (parsedConfig.defaultProvider && typeof parsedConfig.defaultProvider !== 'string') {
                         throw new Error("Invalid ai_config.json structure: 'defaultProvider' must be a string if present.");
                     }
                     // Further validation of AiProviderConfig structure within providers
                     for (const [name, cfg] of Object.entries(parsedConfig.providers)) {
                         if (!cfg || typeof cfg !== 'object' || typeof cfg.provider !== 'string' || !cfg.provider) {
                             throw new Error(`Invalid config for provider "${name}": Missing or invalid 'provider' name.`);
                         }
                         // Add checks for model, apiKey, apiKeyEnvVar, temperature types if needed
                     }
                } else if (file === 'provider_models.toml') {
                    const TOML = (await import('@ltd/j-toml')).default;
                    const parsedToml = TOML.parse(content, { joiner: '\n', bigint: false });
                    if (typeof parsedToml !== 'object' || parsedToml === null) {
                        throw new Error("Invalid TOML structure: Must parse to an object.");
                    }
                    // Validate ProviderModelsStructure
                    for (const [key, value] of Object.entries(parsedToml)) {
                        if (typeof value !== 'object' || value === null || !Array.isArray((value as any).models)) {
                            throw new Error(`Invalid structure for provider "${key}" in TOML: Expected object with 'models' array.`);
                        }
                        if (!(value as any).models.every((m: unknown) => typeof m === 'string')) {
                             throw new Error(`Invalid model list for provider "${key}" in TOML: Contains non-string elements.`);
                        }
                    }
                }
            } catch (validationError) {
                 return res.status(400).json({ error: `Invalid configuration format for ${file}: ${validationError instanceof Error ? validationError.message : String(validationError)}` });
            }


            // Save the file
            await fs.writeFile(filePath, content, 'utf-8');

            // Determine if restart is needed (only provider_models.toml doesn't require one)
            const needsRestart = file !== 'provider_models.toml';

            res.json({
                success: true,
                message: needsRestart
                    ? 'Configuration saved. A restart may be required for changes to take effect.'
                    : 'Configuration saved successfully.',
                needsRestart
            });
        } catch (error) {
            res.status(500).json({ error: `Failed to save configuration file: ${error instanceof Error ? error.message : String(error)}` });
        }
    }) as RequestHandler);

    return router;
}
