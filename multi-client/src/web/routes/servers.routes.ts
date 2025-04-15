import { Router, type Request, type Response } from 'express'; // Import Request and Response types
import type { ServerManager } from '../../ServerManager.js';
import * as fs from 'node:fs/promises';
import * as fsSync from 'node:fs'; // For existsSync
import * as path from 'node:path';
import { fileURLToPath } from 'node:url';
import type { ConfigFileStructure } from '../../types.js';

// Helper for paths
const __filename = fileURLToPath(import.meta.url);
// Correct baseDir calculation: up 3 levels from dist/src/web/routes/
const routesDir = path.dirname(__filename);
const baseDir = path.join(routesDir, '../../..'); // Project root

export function createServersRouter(serverManager: ServerManager): Router {
    const router = Router();
    const serversConfigPath = path.join(baseDir, 'servers.json');

    // --- Get Connected Server Status & Tools ---
    router.get('/', async (req: Request, res: Response) => { // Add types
        try {
            // Get status from ServerManager (assuming it has a method like getServerStatuses)
            // For now, just return connected names as before, but ideally return status objects
            const connectedServers = serverManager.getConnectedServers(); // Replace with getStatuses if available
            // TODO: Enhance this to return status objects [{ name, status, error? }]
            // For now, map names to a basic status object for consistency
            const serverStatuses = connectedServers.map(name => ({ name, status: 'connected' }));
            res.json({ servers: serverStatuses });
        } catch (error) {
            res.status(500).json({ error: `Failed to get server status: ${error instanceof Error ? error.message : String(error)}` });
        }
    });

    router.get('/tools', async (req: Request, res: Response) => { // Add types
        try {
            const tools = await serverManager.getAllTools();
            // Group tools by server here
            const toolsByServer: Record<string, any[]> = {};
            for (const tool of tools) {
                // Need a way to know which server provided the tool.
                // Assuming ServerManager.getAllTools() returns tools with server info,
                // or we modify ServerManager.findToolProvider to return server for each tool.
                // For now, let's assume a placeholder server or group all under 'all'.
                const serverName = serverManager.findToolProvider(tool.name) || 'unknown_server';
                if (!toolsByServer[serverName]) {
                    toolsByServer[serverName] = [];
                }
                // Avoid adding duplicates if multiple servers provide the same tool name
                if (!toolsByServer[serverName].some(t => t.name === tool.name)) {
                    toolsByServer[serverName].push(tool);
                }
            }
            res.json({ tools: toolsByServer }); // Send grouped tools
        } catch (error) {
            res.status(500).json({ error: `Failed to get tools: ${error instanceof Error ? error.message : String(error)}` });
        }
    });

    // --- Get Server Configuration File (servers.json) ---
    // This is used by the modal to populate the editor
    router.get('/config', async (req: Request, res: Response) => { // Add types
        try {
            if (!fsSync.existsSync(serversConfigPath)) {
                // Return empty structure if file doesn't exist
                return res.json({ mcpServers: {} });
            }
            const serversConfigFile = await fs.readFile(serversConfigPath, 'utf-8');
            const serversConfig = JSON.parse(serversConfigFile);
            res.json(serversConfig);
        } catch (error) {
            res.status(500).json({ error: `Failed to get server configurations: ${error instanceof Error ? error.message : String(error)}` });
        }
    });

    // --- Update Server Configuration File (servers.json) ---
    // This is called when saving changes in the modal
    router.post('/config', async (req: Request, res: Response) => { // Add types
        try {
            const { config } = req.body; // Expecting the full { mcpServers: {...} } structure

            if (!config || typeof config !== 'object' || !config.mcpServers) {
                return res.status(400).json({ error: 'Invalid server configuration format. Must include mcpServers object.' });
            }

            // Basic validation (could be more thorough with Zod)
            if (typeof config.mcpServers !== 'object') {
                return res.status(400).json({ error: 'mcpServers must be an object.' });
            }
            for (const [name, cfg] of Object.entries(config.mcpServers as Record<string, any>)) {
                if (!cfg || typeof cfg !== 'object' || !cfg.command) {
                     return res.status(400).json({ error: `Invalid config for server "${name}". Missing command.` });
                }
                // Add more checks for args, env types if needed
                if (cfg.args && !Array.isArray(cfg.args)) {
                    return res.status(400).json({ error: `Invalid config for server "${name}". Args must be an array.` });
                }
                 if (cfg.env && typeof cfg.env !== 'object') {
                    return res.status(400).json({ error: `Invalid config for server "${name}". Env must be an object.` });
                }
            }


            // Save the configuration
            await fs.writeFile(serversConfigPath, JSON.stringify(config, null, 2), 'utf-8');

            res.json({
                success: true,
                message: 'Server configuration updated. Restart the application to apply changes.'
            });
        } catch (error) {
            res.status(500).json({ error: `Failed to update server configurations: ${error instanceof Error ? error.message : String(error)}` });
        }
    });


    return router;
}
