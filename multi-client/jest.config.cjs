/** @type {import('@jest/types').Config.InitialOptions} */
module.exports = {
  preset: 'ts-jest',
  testEnvironment: 'node',
  roots: ['<rootDir>/src'],
  testMatch: ['**/?(*.)+(spec|test).[tj]s?(x)'],
  moduleFileExtensions: ['ts', 'js', 'json', 'node'],
  // NOTE: No custom moduleNameMapper configured – CSS changes don't affect TS tests.
  globals: {
    'ts-jest': {
      tsconfig: 'tsconfig.json',
    },
  },
};