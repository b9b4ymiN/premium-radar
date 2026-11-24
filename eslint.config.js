import js from '@eslint/js'
import globals from 'globals'
import tseslint from 'typescript-eslint'
import reactHooks from 'eslint-plugin-react-hooks'

export default tseslint.config(
  { ignores: ['dist', 'target', 'src-tauri/target', 'src-tauri/gen'] },
  js.configs.recommended,
  ...tseslint.configs.recommended,
  {
    files: ['**/*.{ts,tsx}'],
    languageOptions: { globals: globals.browser },
    plugins: { 'react-hooks': reactHooks },
    rules: {
      ...reactHooks.configs.recommended.rules,
      // fetch/reset effects must set loading/error state synchronously on start; moving to event handlers or external stores would change render timing
      'react-hooks/set-state-in-effect': 'off',
    },
  },
  {
    files: ['tools/**/*.mjs'],
    languageOptions: { globals: globals.node },
  },
)
