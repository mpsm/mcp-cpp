import { TestProject } from '../framework/TestProject.js';

export default async function createEmptyProjectFixture(
  project: TestProject
): Promise<void> {
  // Create an empty directory - no CMakeLists.txt
  await project.writeFile('README.md', 'This is not a CMake project');
  await project.writeFile('some-file.txt', 'Random content');
}
