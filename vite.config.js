import { defineConfig } from "vite";
import { resolve } from "path";
import autoprefixer from "autoprefixer";
import cssnano from "cssnano";

export default defineConfig({
	build: {
		outDir: "dist",
		rollupOptions: {
			input: {
				main: resolve(__dirname, "./assets/main.js"),
				styles: resolve(__dirname, "./assets/styles.css"),
			},
			output: {
				entryFileNames: "[name].js",
				chunkFileNames: "[name].js",
				assetFileNames: "[name].[ext]",
			},
		},
		minify: "terser",
		terserOptions: {
			compress: {
				drop_console: true,
				drop_debugger: true,
				passes: 3,
			},
		},
		cssMinify: true,
	},
	css: {
		postcss: {
			plugins: [
				autoprefixer(),
				cssnano({
					preset: [
						"default",
						{
							discardComments: { removeAll: true },
							normalizeWhitespace: true,
						},
					],
				}),
			],
		},
	},
});
