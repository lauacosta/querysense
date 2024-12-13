document.addEventListener("DOMContentLoaded", () => {
	initHistorial();
	initKeyboard();
	updateForm();
	hideElements();
	initPagination();
	initCsv();
	initSlider();
});

function initHistorial() {
	const historialItems = document.querySelectorAll(".historial-item");
	console.log(historialItems);
	for (const item of historialItems) {
		item.addEventListener("click", () => {
			const queryContent = item.textContent || "";
			document.getElementById("search-input").value = queryContent.trim();
		});
	}
}

/**
 * Inicializa los atajos de teclado para la página.
 */
function initKeyboard() {
	document.addEventListener(
		"keydown",
		(event) => {
			const input = document.getElementById("search-input");
			if (event.ctrlKey && event.key === "b") {
				event.preventDefault();
				input.focus();
			}
		},
		false,
	);
}

/**
 * Inicializa la paginación para una tabla de clase 'modern-table'.
 */
function initPagination() {
	const content = document.querySelector(".modern-table");
	if (!content) {
		return;
	}
	const itemsPerPage = 10;
	let currentPage = 0;
	const items = Array.from(content.getElementsByTagName("tr")).slice(1);
	const totalPages = Math.ceil(items.length / itemsPerPage);
	const pagination_container = document.querySelector(".pagination");
	create_pagination_controls(pagination_container, totalPages, show_page);

	function show_page(page) {
		const startIndex = page * itemsPerPage;
		const endIndex = startIndex + itemsPerPage;

		items.forEach((item, index) => {
			item.style.display =
				index >= startIndex && index < endIndex ? "" : "none";
		});

		update_pagination_info(page, totalPages);
	}

	show_page(currentPage);

	function create_pagination_controls(container, total, show_page_callback) {
		const prevButton = document.createElement("button");
		prevButton.textContent = "<";
		prevButton.addEventListener("click", () => {
			if (currentPage > 0) {
				show_page_callback(--currentPage);
			}
		});

		const pageInfo = document.createElement("span");
		pageInfo.classList.add("page-info");

		const nextButton = document.createElement("button");
		nextButton.textContent = ">";
		nextButton.addEventListener("click", () => {
			if (currentPage < totalPages - 1) {
				show_page_callback(++currentPage);
			}
		});

		container.append(prevButton, pageInfo, nextButton);
		update_pagination_info(currentPage, total);
	}

	function update_pagination_info(page, total) {
		const pageInfo = document.querySelector(".page-info");
		pageInfo.textContent = `Página ${page + 1} de ${total}`;
	}
}
/**
 * Convierte la tabla dada en formato CSV.
 * @param {string} table_id - ID de la tabla a convertir.
 * @returns {string} - Un string con formato CSV.
 */
function table_to_csv(table_id) {
	const table = document.getElementById(table_id);
	const rows = table.querySelectorAll("tr");
	let csvContent = "";

	for (const row of rows) {
		const cells = row.querySelectorAll("td.csv");
		const rowData = Array.from(cells, (cell) => cell.textContent);
		csvContent += `${rowData.join(",")}\n`;
	}

	return csvContent;
}
/**
 * Inicia la descargar de un archivo CSV.
 * @param {string} content - El contenido ha ser descargado.
 * @param {string} fileName - Nombre del archivo.
 */
function download_csv(content, file_name) {
	const blob = new Blob([content], { type: "text/csv" });
	const url = URL.createObjectURL(blob);
	const a = document.createElement("a");
	a.href = url;
	a.download = file_name;
	document.body.appendChild(a);
	a.click();
	document.body.removeChild(a);
	URL.revokeObjectURL(url);
}

/**
 * Inicializa la funcionalidad para descargar una tabla con id 'table-content' como CSV.
 */
function initCsv() {
	const trigger = document.getElementById("csv_trigger");

	if (!trigger) {
		return;
	}

	trigger.addEventListener("click", () => {
		const csv_content = table_to_csv("table-content");
		download_csv(csv_content, "datos-busqueda.csv");
	});
}

function hideElements() {
	const ocultables = document.querySelectorAll(".ocultable");
	const strategy = document.getElementById("strategy");
	const balance_slider = document.querySelector(".balance-slider");

	// Initial setup based on the current value of strategy
	if (strategy.value === "ReciprocalRankFusion") {
		balance_slider.style.display = "block";
		for (const item of ocultables) {
			item.style.display = "block";
		}
	} else {
		balance_slider.style.display = "none";
		if (strategy.value === "Fts") {
			for (const item of ocultables) {
				item.style.display = "none";
			}
		} else {
			for (const item of ocultables) {
				item.style.display = "block";
			}
		}
	}

	// Attach event listener
	strategy.addEventListener("change", () => {
		if (strategy.value === "ReciprocalRankFusion") {
			balance_slider.style.display = "block";
			for (const item of ocultables) {
				item.style.display = "block";
			}
		} else {
			balance_slider.style.display = "none";
			if (strategy.value === "Fts") {
				for (const item of ocultables) {
					item.style.display = "none";
				}
			} else {
				for (const item of ocultables) {
					item.style.display = "block";
				}
			}
		}
	});
}

/**
 * Inicializa el form para la búsqueda, pre-completando valores a partir de la URL si es posible.
 */
function updateForm() {
	const searchConfig = getUrlParams();

	if (Object.keys(searchConfig).length === 0) {
		return;
	}

	document.getElementById("search-input").value = searchConfig.query;
	document.getElementById("age_min").value = searchConfig.edad_min;
	document.getElementById("age_max").value = searchConfig.edad_max;
	document.getElementById("balanceSlider").value = searchConfig.peso_fts || 50;
	document.getElementById("vecinos").value = searchConfig.k;

	const strategy = document.getElementById("strategy");
	strategy.value = searchConfig.strategy;

	document.getElementById("value1Display").textContent = searchConfig.peso_fts;
	document.getElementById("value2Display").textContent =
		searchConfig.peso_semantic;

	document.getElementById("hiddenValue1").value = searchConfig.peso_fts;
	document.getElementById("hiddenValue2").value = searchConfig.peso_semantic;

	const sexoRadios = document.getElementsByName("sexo");
	for (const radio of sexoRadios) {
		if (radio.value === searchConfig.sexo) {
			radio.checked = true;
		}
	}
}

/**
 *  Parsea los parámetros URL y los devuelve como un objeto.
 */
function getUrlParams() {
	const params = new URLSearchParams(window.location.search);
	const searchConfig = {};
	for (const [key, value] of params) {
		searchConfig[key] = value;
	}
	return searchConfig;
}

function initSlider() {
	const balance_slider = document.getElementById("balanceSlider");
	const peso_fts_label = document.getElementById("value1Display");
	const peso_semantic_label = document.getElementById("value2Display");
	const hiddenValue1 = document.getElementById("hiddenValue1");
	const hiddenValue2 = document.getElementById("hiddenValue2");

	if (
		!balance_slider ||
		!peso_fts_label ||
		!peso_semantic_label ||
		!hiddenValue2 ||
		!hiddenValue1
	) {
		return;
	}

	const updateValues = () => {
		const variable1Value = balance_slider.value;
		const variable2Value = 100 - variable1Value;

		peso_fts_label.textContent = variable1Value;
		peso_semantic_label.textContent = variable2Value;

		hiddenValue1.value = variable1Value;
		hiddenValue2.value = variable2Value;
	};

	balance_slider.addEventListener("input", updateValues);
}
