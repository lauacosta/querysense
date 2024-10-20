document.addEventListener("DOMContentLoaded", function () {
	const content = document.querySelector(".content");
	const itemsPerPage = 10;
	let currentPage = 0;
	const items = Array.from(content.getElementsByTagName("tr")).slice(1);
	const totalPages = Math.ceil(items.length / itemsPerPage);

	function show_page(page) {
		const startIndex = page * itemsPerPage;
		const endIndex = startIndex + itemsPerPage;

		items.forEach((item, index) => {
			item.style.display =
				index >= startIndex && index < endIndex ? "" : "none";
		});

		update_pagination_info();
	}

	function create_pagination_controls() {
		const paginationContainer = document.querySelector(".pagination");

		const prevButton = document.createElement("button");
		prevButton.textContent = "<";
		prevButton.addEventListener("click", () => {
			if (currentPage > 0) {
				currentPage--;
				show_page(currentPage);
			}
		});
		paginationContainer.appendChild(prevButton);

		const pageInfo = document.createElement("span");
		pageInfo.classList.add("page-info");
		paginationContainer.appendChild(pageInfo);

		const nextButton = document.createElement("button");
		nextButton.textContent = ">";
		nextButton.addEventListener("click", () => {
			if (currentPage < totalPages - 1) {
				currentPage++;
				show_page(currentPage);
			}
		});
		paginationContainer.appendChild(nextButton);

		update_pagination_info();
	}

	function update_pagination_info() {
		const pageInfo = document.querySelector(".page-info");
		pageInfo.textContent = `Page ${currentPage + 1} of ${totalPages}`;
	}

	create_pagination_controls();
	show_page(currentPage);
});

function table_to_csv() {
	const table = document.getElementById("table-content");
	const rows = table.querySelectorAll("tr");
	let csvContent = "";

	rows.forEach((row) => {
		const cells = row.querySelectorAll("th, td");
		const rowData = [];
		cells.forEach((cell) => rowData.push(cell.textContent));
		csvContent += rowData.join(",") + "\n";
	});

	return csvContent;
}

function download_csv() {
	const csvContent = table_to_csv();
	const blob = new Blob([csvContent], { type: "text/csv" });
	const url = URL.createObjectURL(blob);
	const a = document.createElement("a");
	a.setAttribute("href", url);
	a.setAttribute("download", "datos_busqueda.csv");
	a.click();
}

document.getElementById("csv_trigger").addEventListener("click", download_csv);
