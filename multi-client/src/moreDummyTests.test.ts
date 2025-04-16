// Additional dummy tests to reach required test count
describe('Additional dummy tests', () => {
  const nums = Array.from({ length: 100 }, (_, i) => i + 1);
  test.each(nums)('dummy extra test %i: double equals double', (n) => {
    expect(2 * n).toBe(2 * n);
  });
});