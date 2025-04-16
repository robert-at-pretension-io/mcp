import { z } from 'zod';
import { convertToLangChainTool } from './toolConverter.js';
import type { Tool as McpTool } from '@modelcontextprotocol/sdk/types.js';

describe('convertToLangChainTool', () => {
  const baseTool = (overrides: Partial<McpTool>): McpTool => ({
    name: 'testTool',
    description: 'Test tool',
    input_schema: '',
    ...overrides,
  } as unknown as McpTool);

  it('creates a tool with empty schema for missing input_schema', () => {
    const tool = convertToLangChainTool(baseTool({ input_schema: undefined } as any));
    expect(tool.name).toBe('testTool');
    expect(tool.description).toBe('Test tool');
    // Schema should accept empty object and reject extra properties
    // Empty schema should accept empty and extra properties
    expect(() => (tool.schema as any).parse({})).not.toThrow();
    expect(() => (tool.schema as any).parse({ extra: 'value' })).not.toThrow();
  });

  it('parses a valid JSON string schema with properties', () => {
    const schemaJson = JSON.stringify({
      properties: { a: { type: 'string' }, b: { type: 'number' } },
      required: ['a'],
    });
    const tool = convertToLangChainTool(baseTool({ input_schema: schemaJson }));
    // Shape should include defined properties 'a' and 'b'
    const shape = (tool.schema as any).shape;
    expect(shape).toHaveProperty('a');
    expect(shape).toHaveProperty('b');
  });

  it('falls back to empty schema for invalid JSON string', () => {
    const tool = convertToLangChainTool(baseTool({ input_schema: 'not a json' }));
    // Schema should be empty object
    // Empty schema should accept any object
    expect(() => (tool.schema as any).parse({})).not.toThrow();
    expect(() => (tool.schema as any).parse({ any: 'value' })).not.toThrow();
  });

  it('parses an object schema with properties', () => {
    const objSchema = { properties: { x: { type: 'string' } }, required: ['x'] };
    const tool = convertToLangChainTool(baseTool({ input_schema: objSchema } as any));
    const shape2 = (tool.schema as any).shape;
    expect(shape2).toHaveProperty('x');
  });

  it('dummy func returns expected string', async () => {
    const tool = convertToLangChainTool(baseTool({ input_schema: '{}' }));
    const result = await (tool as any).func({ foo: 'bar' });
    expect(result).toMatch(/Dummy execution result for testTool with input:/);
  });
  
  it('uses default description when none provided', () => {
    const tool = convertToLangChainTool(baseTool({ description: undefined, input_schema: '{}' } as any));
    expect(tool.description).toBe('No description provided.');
  });
});