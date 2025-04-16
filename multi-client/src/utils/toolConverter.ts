import { z } from 'zod';
import { DynamicStructuredTool } from '@langchain/core/tools';
import type { Tool as McpTool } from '@modelcontextprotocol/sdk/types.js'; // MCP Tool type
import type { StructuredToolInterface } from '@langchain/core/tools';

/**
 * Converts an MCP Tool definition into a LangChain StructuredToolInterface.
 *
 * Note: The actual execution logic (`func`) is not needed here, as the
 * ConversationManager handles calling the MCP server. We just need the schema.
 * LangChain uses the schema to inform the LLM about the tool's arguments.
 */
export function convertToLangChainTool(mcpTool: McpTool): StructuredToolInterface {
  let inputSchema: z.ZodObject<any>;

  try {
    // Attempt to parse the input_schema string into a Zod schema object
    // This assumes the schema is a valid JSON representation of a Zod schema
    // or at least a basic JSON schema that Zod can infer.
    // A more robust solution might involve a dedicated JSON Schema -> Zod converter.
    if (typeof mcpTool.input_schema === 'string') {
        // Basic check: if it looks like JSON, try parsing
        if (mcpTool.input_schema.trim().startsWith('{')) {
            const schemaJson = JSON.parse(mcpTool.input_schema);
            // Zod can often infer from a basic JSON schema structure
            inputSchema = z.object(schemaJson.properties || {});
             // Add descriptions to properties if available
             if (schemaJson.properties) {
                Object.keys(schemaJson.properties).forEach(key => {
                    if (schemaJson.properties[key].description && inputSchema.shape[key]) {
                        inputSchema.shape[key] = inputSchema.shape[key].describe(schemaJson.properties[key].description);
                    }
                });
            }
            // Mark required fields
            if (Array.isArray(schemaJson.required)) {
                schemaJson.required.forEach((key: string) => {
                    if (inputSchema.shape[key]) {
                        // Make optional fields required - Zod doesn't directly modify optionality easily after creation
                        // This might require rebuilding the schema object if strict optionality is needed.
                        // For now, we rely on the LLM understanding the schema description.
                    }
                });
            }

        } else {
             console.warn(`[ToolConverter] Non-JSON schema for tool "${mcpTool.name}". Using empty schema. Schema was: ${mcpTool.input_schema}`);
             inputSchema = z.object({}); // Fallback for non-JSON string schemas
        }

    } else if (typeof mcpTool.input_schema === 'object' && mcpTool.input_schema !== null) {
      // If it's already an object (potentially JSON schema)
       inputSchema = z.object((mcpTool.input_schema as any).properties || {});
        // Add descriptions and handle required fields as above
        if ((mcpTool.input_schema as any).properties) {
            Object.keys((mcpTool.input_schema as any).properties).forEach(key => {
                if ((mcpTool.input_schema as any).properties[key].description && inputSchema.shape[key]) {
                    inputSchema.shape[key] = inputSchema.shape[key].describe((mcpTool.input_schema as any).properties[key].description);
                }
            });
        }
         if (Array.isArray((mcpTool.input_schema as any).required)) {
            (mcpTool.input_schema as any).required.forEach((key: string) => {
                 if (inputSchema.shape[key]) { /* Mark as required if possible */ }
            });
        }

    } else {
      // Default to an empty schema if input_schema is missing or invalid
      inputSchema = z.object({});
    }
  } catch (error) {
    console.error(`[ToolConverter] Error parsing schema for tool "${mcpTool.name}":`, error);
    inputSchema = z.object({}); // Fallback on error
  }

  return new DynamicStructuredTool({
    name: mcpTool.name,
    description: mcpTool.description || 'No description provided.',
    schema: inputSchema,
    func: async (input) => {
      // This function won't actually be called by our setup.
      // ConversationManager intercepts the tool call request from the LLM
      // and calls the MCP server directly.
      // We provide a dummy implementation to satisfy the interface.
      console.warn(`[ToolConverter] Dummy func called for ${mcpTool.name}. This should not happen in normal operation.`);
      return `Dummy execution result for ${mcpTool.name} with input: ${JSON.stringify(input)}`;
    },
  });
}
