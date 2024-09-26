<script setup lang="ts">
import type { TableRow } from '../types/TableRow'

defineProps<{
    props: TableRow[]
}>()
</script>
<template>
    <div id="scrollHandler">
        <table id="Tabla">
            <thead>
                <tr>
                    <th scope="col">Puntaje</th>
                    <th scope="col">Correo</th>
                    <th scope="col">Edad</th>
                    <th scope="col">Ciudad</th>
                    <th scope="col">Provincia</th>
                    <th scope="col">Estudios</th>
                    <th scope="col">Experiencia</th>
                    <th scope="col">Descripcion</th>
                    <th scope="col">Est Recientes</th>
                </tr>
            </thead>
            <tbody>
                <tr v-for="(row, _) in props" :key="row.email">
                    <td>{{ row.ranking_score }} </td>
                    <td> {{ row.email }} </td>
                    <td> {{ row.edad }} </td>
                    <td> {{ row.ciudad }} </td>
                    <td> {{ row.provincia }} </td>
                    <td> {{ row.estudios }} </td>
                    <td v-html="row.experiencia"></td>
                    <td v-html="row.descripcion"></td>
                    <td v-html="row.estudios_mas_recientes"></td>
                </tr>
            </tbody>
        </table>
    </div>
</template>

<style>
#scrollHandler {
    width: 95%;
    height: 60rem;
    overflow-y: auto;
    overflow-x: auto;
    border: 1px solid #ccc;
}

#Tabla {
    /* table-layout: fixed; */
    width: 100%;
    border-collapse: collapse;
    border: 2px solid rgb(36 36 36);
    font-family: "Roboto Mono", monospace;
    background-color: #e1e1e1;
    text-align: center;
    font-size: 1.1rem;
}

#Tabla th,
#Tabla td {
    border: 3px solid rgb(36 36 36);
    padding: 0.5rem;
}

#Tabla td {
    text-align: justified;
}

#Tabla tr:hover {
    background-color: #f1f1f1;
}

tr.hidden {
    visibility: hidden;
}

#Tabla th {
    position: sticky;
    text-align: center;
    border-collapse: collapse;
    z-index: 10;
    background-color: #242424;
    top: 0;
    color: #e0e0e0;
}
</style>
