import { Router } from 'express';
import * as fs from 'node:fs/promises';
import * as fsSync from 'node:fs'; // For existsSync
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
    router.get('/:file', async (req, res) => {
        try {
            const { file } = req.params;

            if (!allowedFiles.includes(file)) {
                return res.status(403).json({ error: `Access to file "${file}" is not allowed` });
            }

            const filePath = path.join(baseDir, file);

            if (!fsSync.existsSync(filePath)) {
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
    });

    // --- Save Config File Content ---
    router.post('/:file', async (req, res) => {
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

            // Validate content based on file type
            if (file.endsWith('.json')) {
                try {
                    JSON.parse(content);
                } catch (jsonError) {
                    return res.status(400).json({ error: `Invalid JSON: ${jsonError instanceof Error ? jsonError.message : String(jsonError)}` });
                }
            } else if (file.endsWith('.toml')) {
                 try {
                    const TOML = (await import('@ltd/j-toml')).default; // Dynamic import inside async function
                    TOML.parse(content, { joiner: '\n', bigint: false });
                } catch (tomlError) {
                    return res.status(400).json({ error: `Invalid TOML: ${tomlError instanceof Error ? tomlError.message : String(tomlError)}` });
                }
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
    });

    return router;
}
