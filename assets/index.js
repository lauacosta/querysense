document.addEventListener("DOMContentLoaded", function() {
    initKeyboard();
    initForm();
    initPagination();
    initCsv();
});

/** 
 * Inicializa los atajos de teclado para la página.
 */
function initKeyboard() {
    document.addEventListener('keydown', function(event) {
        const input = document.getElementById('query');
        if (event.ctrlKey && event.key === 'b') {
            event.preventDefault();
            input.focus();
        }
    }, false);
}

/** 
 * Inicializa la paginación para una tabla de clase 'modern-table'.
 */
function initPagination() {
    const content = document.querySelector(".modern-table");
    if (!content) {
        return
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

    rows.forEach((row) => {
        const cells = row.querySelectorAll("td.csv");
        const rowData = Array.from(cells, (cell) => cell.textContent);
        csvContent += rowData.join(",") + "\n";
    });

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
    document.getElementById("csv_trigger").addEventListener("click", function() {
        const csv_content = table_to_csv("table-content");
        download_csv(csv_content, "datos-busqueda.csv");
    });

}

/** 
 * Inicializa el form para la búsqueda, pre-completando valores a partir de la URL si es posible.
 */
function initForm() {
    const searchConfig = getUrlParams();
    document.getElementById('query').value = searchConfig.query;
    document.getElementById('strategy').value = searchConfig.strategy;

    const sexoRadios = document.getElementsByName('sexo');
    for (const radio of sexoRadios) {
        if (radio.value === searchConfig.sexo) {
            radio.checked = true;
        }
    }

    document.getElementById('age_min').value = searchConfig.edad_min;
    document.getElementById('age_max').value = searchConfig.edad_max;
}

/** 
 *  Parsea los parámetros URL y los devuelve como un objeto.
 */
function getUrlParams() {
    const params = new URLSearchParams(window.location.search);
    return {
        query: params.get('query') || '',
        strategy: params.get('strategy') || 'Fts',
        sexo: params.get('sexo') || 'U',
        edad_min: params.get('edad_min') || '18',
        edad_max: params.get('edad_max') || '100'
    };
}
